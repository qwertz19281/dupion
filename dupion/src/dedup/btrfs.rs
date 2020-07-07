use super::*;
use std::{sync::atomic::Ordering, fs::{Metadata, File}};
use util::{disp_processed_bytes, disp_processed_files};
use btrfs2::{DedupeRange, FileDescriptor, DedupeRangeDestInfo, DedupeRangeStatus, deduplicate_range};
use std::os::unix::io::FromRawFd;
use size_format::SizeFormatterBinary;

pub struct BtrfsDedup;

impl Deduper for BtrfsDedup {
    fn dedup_groups(&mut self, mut groups: Vec<DedupGroup>, state: &'static RwLock<State>, opts: &'static Opts) -> AnyhowResult<()> {
        // The dedups are split in batches to fit the os cache for readahead
        // the available file cache is estimated by the unused os memory
        // all files in batch will be opened 

        let mut go = 0;

        let mut current = Vec::new();

        let mut cache_info = CacheUsable::new(256*1024*1024);

        let s = state.write();

        while go < groups.len() {
            let cache_max = cache_info.get();
            let mut cache_take = 0;

            // fill batch by cache size measure
            while cache_take < cache_max && go < groups.len() {
                let mut group = groups[go].clone();
                let can_take = group.range.end - group.range.start;
                let old_take = cache_take;

                if old_take + can_take*group.sum > cache_max {
                    // split the group in 2 size ranges, the first for this batch, the last for the next batch
                    let take = (cache_max - cache_take)/group.sum;
                    assert!(take < can_take);

                    group.range = 0..take;
                    groups[go].range.start = take;

                    cache_take = cache_max;
                    current.push(group);
                    break;
                }else{
                    //would fit into cache
                    cache_take += can_take*group.sum;
                    current.push(group);
                    go+=1;
                }
            }

            eprintln!("Batch {} files, {}B",current.len(),SizeFormatterBinary::new(cache_take));
            
            let mut opened: Vec<(DedupGroup,FileDescriptor,Vec<FileDescriptor>)> = Vec::with_capacity(current.len());

            let mut batch_file_sum = 0;

            // open all the relevant files
            'g: for group in current.drain(..) {
                let senpai_fd = {
                    let path = &s.tree[group.senpai].path;
                    eprintln!("\tGroup {}B..{}B {}",SizeFormatterBinary::new(group.range.start),SizeFormatterBinary::new(group.range.end),opts.path_disp(path));
                    let fd = match FileDescriptor::open(
                        path,
                        libc::O_RDWR,
                    ) {
                        Ok(v) => v,
                        Err(e) => {
                            eprintln!("\tError opening for dedup: {} ({})",e,opts.path_disp(path));
                            continue 'g;
                        }
                    };

                    let meta = match fd_metadata(fd.get_value()) {
                        Ok(m) => m,
                        Err(e) => {
                            eprintln!("\tError reading metadata for dedup: {} ({})",e,opts.path_disp(path));
                            continue 'g;
                        }
                    };

                    if meta.len() == group.file_size {
                        fd
                    }else{
                        eprintln!("\tComodified file, skip group: {}",opts.path_disp(path));
                        continue 'g;
                    }
                };

                let mut dups_fd = Vec::with_capacity(group.dups.len());

                for &id in &group.dups {
                    let path = &s.tree[id].path;
                    eprintln!("\t\t{}",opts.path_disp(path));
                    let fd = match FileDescriptor::open(
                        path,
                        libc::O_RDWR,
                    ) {
                        Ok(v) => v,
                        Err(e) => {
                            eprintln!("\tError opening for dedup: {} ({})",e,opts.path_disp(path));
                            continue 'g;
                        }
                    };

                    let meta = match fd_metadata(fd.get_value()) {
                        Ok(m) => m,
                        Err(e) => {
                            eprintln!("\tError reading metadata for dedup: {} ({})",e,opts.path_disp(path));
                            continue 'g;
                        }
                    };

                    if meta.len() == group.file_size {
                        dups_fd.push(fd);
                    }else{
                        eprintln!("\tComodified file, skip group: {}",opts.path_disp(path));
                        continue 'g;
                    }
                }

                assert_eq!(dups_fd.len(),group.dups.len());

                batch_file_sum += group.dups.len()+1;

                opened.push((group,senpai_fd,dups_fd));
            }

            let mut readahead: Vec<(u64,&FileDescriptor,&Range<u64>)> = Vec::with_capacity(batch_file_sum);

            // for readahead, retrieve and sort by the physical pos, then fadvise
            for (group,senpai_fd,dups_fd) in &opened {
                readahead.push((
                    s.tree[group.senpai].phys.unwrap(),
                    senpai_fd,
                    &group.range,
                ));

                for (&id,fd) in group.dups.iter().zip(dups_fd.iter()) {
                    readahead.push((
                        s.tree[id].phys.unwrap(),
                        fd,
                        &group.range,
                    ))
                }
            }

            readahead.sort_by_key(|v| v.0 );

            for (_,fd,range) in &readahead {
                unsafe{
                    libc::posix_fadvise(
                        fd.get_value(),
                        range.start as i64,
                        (range.end - range.start) as i64,
                        libc::POSIX_FADV_SEQUENTIAL,
                    );
                }
            }
            for (_,fd,range) in &readahead {
                unsafe{
                    libc::posix_fadvise(
                        fd.get_value(),
                        range.start as i64,
                        (range.end - range.start) as i64,
                        libc::POSIX_FADV_WILLNEED,
                    );
                }
            }

            // issue dedup_range ioctl for the dup ranges
            for (group,senpai_fd,dups_fd) in &opened {
                let dest_infos: Vec<_> = dups_fd.iter()
                    .map(|fd| DedupeRangeDestInfo{
                        dest_fd: fd.get_value() as i64,
                        dest_offset: group.range.start,
                        bytes_deduped: 0,
                        status: DedupeRangeStatus::Same,
                    })
                    .collect();

                let mut dedup_range = DedupeRange{
                    src_offset: group.range.start,
                    src_length: group.range.end - group.range.start,
                    dest_infos,
                };

                if let Err(e) = deduplicate_range(
                    senpai_fd.get_value(),
                    &mut dedup_range,
                ) {
                    eprintln!("\tError deduplicating: {}",e);
                }

                disp_processed_bytes.fetch_add(group.dups.len() * (group.range.end - group.range.start) as usize,Ordering::Relaxed);
                disp_processed_files.fetch_add(group.dups.len(),Ordering::Relaxed);
            }

        }
        
        Ok(())
    }
}

pub fn fd_metadata(fd: i32) -> std::io::Result<Metadata> {
    let file = unsafe{File::from_raw_fd(fd)};
    let meta = file.metadata();
    std::mem::forget(file);
    meta
}
