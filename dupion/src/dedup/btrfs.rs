use super::*;
use std::{sync::atomic::Ordering, fs::{Metadata, File}};
use util::{DISP_PROCESSED_BYTES, DISP_PROCESSED_FILES};
use ::btrfs::{DedupeRange, DedupeRangeDestInfo, DedupeRangeStatus, deduplicate_range};
use std::os::unix::io::FromRawFd;
use size_format::SizeFormatterBinary;
use fd::FileDescriptor;

pub struct BtrfsDedup;

impl Deduper for BtrfsDedup {
    fn dedup_groups(&mut self, mut groups: Vec<DedupGroup>, state: &'static RwLock<State>, opts: &'static Opts) -> AnyhowResult<()> {
        // The dedups are split in batches to fit the os cache for readahead
        // the available file cache is estimated by the unused os memory
        // all files in batch will be opened 

        let file_split_round = 1024*1024*12;

        let max_dups_per_group = 127; // TODO determine amount of files we can open currently

        let mut gi = 0;

        let mut cache_info = CacheUsable::new(file_split_round*2 .. opts.dedup_budget);

        let mut s = state.write();

        while gi < groups.len() {
            if groups[gi].dups.is_empty() {
                gi += 1;
                continue;
            }

            let cache_max = cache_info.get();

            // 1. Try to fit as much of groups as possible into batch, without splitting group files or range
            {
                let mut current_batch: Vec<(&DedupGroup,bool)> = Vec::new();

                let mut used_in_batch = 0;
                let mut to_open_files = 0;

                while gi < groups.len() && used_in_batch + groups[gi].usage() <= cache_max && groups[gi].sum() + to_open_files <= (max_dups_per_group as u64 + 1) {
                    used_in_batch += groups[gi].usage();
                    to_open_files += groups[gi].sum();
                    current_batch.push((&groups[gi],true));
                    gi += 1;
                    while gi < groups.len() && groups[gi].dups.is_empty() {
                        gi += 1;
                    }
                }

                // did we do it? submit, else continue
                if !current_batch.is_empty() {
                    dedup_group_batch(&current_batch, &mut s, opts, used_in_batch)?;
                    current_batch.clear();
                    continue;
                }

                // just for sure
                if gi >= groups.len() {
                    break;
                }
            }

            // 2. If the group has too many files in it
            if groups[gi].dups.len() > max_dups_per_group {
                let first_half = groups[gi].split_off_start_at_candidate_n(max_dups_per_group);
                dedup_group_batch(&[(&first_half,true)], &mut s, opts, first_half.usage())?;
                continue;
            }

            // 3. Try to split a dedup group by file to fit in
            {
                let group = &mut groups[gi];
                assert!(group.usage() > cache_max);

                // the n of dup files that would fit into cache
                let max_group_files = cache_max / group.range_len();

                if max_group_files >= 2 {
                    assert!(max_group_files <= group.sum());
                    assert!(!group.dups.is_empty());
                    // We can split
                    let first_half = group.split_off_start_at_candidate_n(max_group_files as usize - 1);
                    dedup_group_batch(&[(&first_half,true)], &mut s, opts, first_half.usage())?;
                } else {
                    // 4. Try to split in dedup range dimension, currently very aggressively only on one dup
                    assert!(!group.dups.is_empty());

                    let mut group = group.split_off_start_at_candidate_n(1);
                    assert!(group.usage() > cache_max);

                    let max_range_size = cache_max / group.sum(); // group.sum == 2 at this point
                    let max_range_size = max_range_size/file_split_round*file_split_round;
                    let max_range_size = max_range_size.min(group.range_len());
                    assert!(max_range_size*group.sum() <= cache_max);
                    assert!(max_range_size*group.sum() < group.usage());
                    let max_range_size = max_range_size.max(file_split_round);

                    while !group.range.is_empty() {
                        let first_half = group.split_off_start_range(max_range_size);

                        let is_last_part = group.range.is_empty();

                        dedup_group_batch(&[(&first_half,is_last_part)], &mut s, opts, first_half.usage())?;
                    }
                }
            }
        }
        
        Ok(())
    }
}

pub fn dedup_group_batch(current: &[(&DedupGroup,bool)], state: &mut State, opts: &'static Opts, batch_size: u64) -> AnyhowResult<()> {
    let real = !opts.dedup_simulate;

    if opts.verbose {
        dprintln!(
            "Batch {} groups with {} dups, {}B",
            current.len(),
            current.iter()
                .map(|(g,_)| g.dups.len() )
                .sum::<usize>(),
            SizeFormatterBinary::new(batch_size)
        );
    }

    if opts.dedup_simulate {
        for &(group,last_part) in current {
            if group.dups.is_empty() {
                continue;
            }

            if opts.verbose {
                dprintln!(
                    "\tGroup {}B..{}B -> {} ({})",
                    SizeFormatterBinary::new(group.range.start),
                    SizeFormatterBinary::new(group.range.end),
                    opts.path_disp(&state.tree[group.senpai].path),
                    group.dups.len()+1,
                );
            }

            DISP_PROCESSED_BYTES.fetch_add(group.dups.len() as u64 * (group.range.end - group.range.start),Ordering::Relaxed);
            if last_part {
                DISP_PROCESSED_FILES.fetch_add(group.dups.len() as u64,Ordering::Relaxed);
            }
        }
        return Ok(());
    }
            
    let mut opened: Vec<(DedupGroup,FileDescriptor,Vec<FileDescriptor>,bool)> = Vec::with_capacity(current.len());

    let mut batch_file_sum = 0;

    let open_dup = |group: &DedupGroup,id: VfsId| {
        let path = &state.tree[id].path;
        let fd = match FileDescriptor::open(
            path,
            libc::O_RDONLY,
        ) {
            Ok(v) => v,
            Err(e) => {
                dprintln!("\tError opening for dedup: {} ({})",e,opts.path_disp(path));
                return Err(());
            }
        };

        let meta = match fd_metadata(fd.get_value()) {
            Ok(m) => m,
            Err(e) => {
                dprintln!("\tError reading metadata for dedup: {} ({})",e,opts.path_disp(path));
                return Err(());
            }
        };

        if meta.len() == group.actual_file_size {
            Ok(fd)
        }else{
            dprintln!("\tComodified file, skip group: {}",opts.path_disp(path));
            Err(())
        }
    };

    // open all the relevant files
    'g: for &(group,last_part) in current {
        if group.dups.is_empty() {
            continue;
        }

        if opts.verbose {
            dprintln!(
                "\tGroup {}B..{}B -> {} ({})",
                SizeFormatterBinary::new(group.range.start),
                SizeFormatterBinary::new(group.range.end),
                opts.path_disp(&state.tree[group.senpai].path),
                group.dups.len()+1,
            );
        }

        let senpai_fd = match open_dup(&group,group.senpai) {
            Ok(v) => v,
            Err(_) => continue 'g,
        };

        let mut group = group.clone();

        let mut dups_fd = Vec::with_capacity(group.dups.len());

        let mut i = 0;
        while i < group.dups.len() {
            if let Ok(fd) = open_dup(&group,group.dups[i]) {
                dups_fd.push(fd);
                i += 1;
            } else {
                group.dups.remove(i);
            }
        }

        assert_eq!(dups_fd.len(),group.dups.len());

        batch_file_sum += group.dups.len()+1;

        opened.push((group,senpai_fd,dups_fd,last_part));
    }

    let mut readahead: Vec<(u64,&FileDescriptor,&Range<u64>)> = Vec::with_capacity(batch_file_sum);

    // for readahead, retrieve and sort by the physical pos, then fadvise
    for (group,senpai_fd,dups_fd,_) in &opened {
        readahead.push((
            state.tree[group.senpai].phys.unwrap(),
            senpai_fd,
            &group.range,
        ));

        for (&id,fd) in group.dups.iter().zip(dups_fd.iter()) {
            readahead.push((
                state.tree[id].phys.unwrap(),
                fd,
                &group.range,
            ))
        }
    }

    readahead.sort_by_key(|&(phys,..)| phys );

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
    for (group,senpai_fd,dups_fd,last_part) in opened {
        assert_eq!(dups_fd.len(),group.dups.len());
        let senpai_path = &state.tree[group.senpai].path;

        if opts.verbose {
            dprintln!(
                "\tDedup {}B..{}B -> {} ({})",
                SizeFormatterBinary::new(group.range.start),
                SizeFormatterBinary::new(group.range.end),
                opts.path_disp(senpai_path),
                dups_fd.len()+1,
            );
        }

        let dest_infos: Vec<_> = dups_fd.iter()
            .map(|fd| DedupeRangeDestInfo {
                dest_fd: fd.get_value() as i64,
                dest_offset: group.range.start,
                bytes_deduped: 0,
                status: DedupeRangeStatus::Same,
            })
            .collect();

        let mut dedup_range = DedupeRange {
            src_offset: group.range.start,
            src_length: group.range.end - group.range.start,
            dest_infos,
        };

        if real {
            if let Err(e) = deduplicate_range(
                senpai_fd.get_value(),
                &mut dedup_range,
            ) {
                dprintln!("\tError deduplicating: {}",e);
            }
        }

        DISP_PROCESSED_BYTES.fetch_add(group.dups.len() as u64 * (group.range.end - group.range.start),Ordering::Relaxed);
        if last_part {
            DISP_PROCESSED_FILES.fetch_add(group.dups.len() as u64,Ordering::Relaxed);
        }

        let mut deduped = 0;

        for (i,&id) in dedup_range.dest_infos.iter().zip(group.dups.iter()) {
            deduped += i.bytes_deduped;
            let path = &state.tree[id].path;
            if i.status == DedupeRangeStatus::Differs {
                dprintln!("\t\tNot deduped {}",opts.path_disp(path));
            } else {
                state.tree[id].dedup_state = Some(true);
            }
        }

        DISP_DEDUPED_BYTES.fetch_add(deduped,Ordering::Relaxed);
    }

    Ok(())
}

pub fn fd_metadata(fd: i32) -> std::io::Result<Metadata> {
    let file = unsafe{File::from_raw_fd(fd)};
    let meta = file.metadata();
    std::mem::forget(file);
    meta
}

