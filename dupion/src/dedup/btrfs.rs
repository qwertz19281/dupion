use super::*;
use std::{sync::atomic::Ordering, fs::{Metadata, File}};
use util::{disp_processed_bytes, disp_processed_files};
use btrfs2::{DedupeRange, DedupeRangeDestInfo, DedupeRangeStatus, deduplicate_range};
use std::os::unix::io::FromRawFd;
use size_format::SizeFormatterBinary;
use fd::FileDescriptor;

pub struct BtrfsDedup;

impl Deduper for BtrfsDedup {
    fn dedup_groups(&mut self, mut groups: Vec<DedupGroup>, state: &'static RwLock<State>, opts: &'static Opts) -> AnyhowResult<()> {
        // The dedups are split in batches to fit the os cache for readahead
        // the available file cache is estimated by the unused os memory
        // all files in batch will be opened 

        let real = true;

        let mut go = 0;

        let mut current = Vec::new();

        let mut cache_info = CacheUsable::new(256*1024*1024);

        let s = state.write();

        while go < groups.len() {
            let cache_max = cache_info.get();
            let mut cache_take = 0;
            let open_max = 512;
            let mut open_take = 0;

            //eprintln!("Acq OSC {} {}B",current.len(),SizeFormatterBinary::new(cache_max));

            if groups[go].file_size*groups[go].sum < cache_max {
                groups[go].range.start = 0;
            }

            // fill batch by cache size measure
            while cache_take < cache_max && go < groups.len() {
                let mut group = groups[go].clone();

                open_take += group.sum;
                if open_take > open_max {
                    break;
                }

                let can_take = group.range.end - group.range.start;
                let old_take = cache_take;

                if old_take + can_take*group.sum > cache_max {
                    // split the group in 2 size ranges, the first for this batch, the last for the next batch
                    let take = (cache_max - cache_take)/group.sum;
                    assert!(take < can_take);

                    group.range.end = group.range.start+take;
                    groups[go].range.start = group.range.end;

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

            let open_dup = |group: &DedupGroup,id: VfsId| {
                let path = &s.tree[id].path;
                let fd = match FileDescriptor::open(
                    path,
                    libc::O_RDWR,
                ) {
                    Ok(v) => v,
                    Err(e) => {
                        eprintln!("\tError opening for dedup: {} ({})",e,opts.path_disp(path));
                        return Err(());
                    }
                };

                let meta = match fd_metadata(fd.get_value()) {
                    Ok(m) => m,
                    Err(e) => {
                        eprintln!("\tError reading metadata for dedup: {} ({})",e,opts.path_disp(path));
                        return Err(());
                    }
                };

                if meta.len() == group.file_size {
                    Ok(fd)
                }else{
                    eprintln!("\tComodified file, skip group: {}",opts.path_disp(path));
                    Err(())
                }
            };

            // open all the relevant files
            'g: for group in current.drain(..) {
                eprintln!(
                    "\tGroup {}B..{}B -> {}",
                    SizeFormatterBinary::new(group.range.start),
                    SizeFormatterBinary::new(group.range.end),
                    opts.path_disp(&s.tree[group.senpai].path)
                );
                let senpai_fd = match open_dup(&group,group.senpai) {
                    Ok(v) => v,
                    Err(_) => continue 'g,
                };

                let mut dups_fd = Vec::with_capacity(group.dups.len());

                for &id in &group.dups {
                    match open_dup(&group,id) {
                        Ok(v) => dups_fd.push(v),
                        Err(_) => continue 'g,
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

            if real {
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
            }

            // issue dedup_range ioctl for the dup ranges
            for (group,senpai_fd,dups_fd) in &opened {
                assert_eq!(dups_fd.len(),group.dups.len());
                let senpai_path = &s.tree[group.senpai].path;
                eprintln!("\tDedup {}B..{}B -> {}",SizeFormatterBinary::new(group.range.start),SizeFormatterBinary::new(group.range.end),opts.path_disp(senpai_path));

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

                if real {
                    if let Err(e) = deduplicate_range(
                        senpai_fd.get_value(),
                        &mut dedup_range,
                    ) {
                        eprintln!("\tError deduplicating: {}",e);
                    }
                }

                disp_processed_bytes.fetch_add(group.dups.len() as u64 * (group.range.end - group.range.start),Ordering::Relaxed);
                disp_processed_files.fetch_add(group.dups.len() as u64,Ordering::Relaxed);

                let mut deduped = 0;

                for (i,&id) in dedup_range.dest_infos.iter().zip(group.dups.iter()) {
                    deduped += i.bytes_deduped;
                    let path = &s.tree[id].path;
                    if i.status == DedupeRangeStatus::Differs {
                        eprintln!("\t\tNot deduped {}",opts.path_disp(path));
                    }
                }

                disp_deduped_bytes.fetch_add(deduped,Ordering::Relaxed);
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

