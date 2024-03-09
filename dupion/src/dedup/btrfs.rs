use super::*;
use std::{collections::VecDeque, fs::{Metadata, File}, sync::atomic::Ordering};
use util::{DISP_PROCESSED_BYTES, DISP_PROCESSED_FILES};
use ::btrfs::{DedupeRange, DedupeRangeDestInfo, DedupeRangeStatus, deduplicate_range};
use std::os::unix::io::FromRawFd;
use size_format::SizeFormatterBinary;
use fd::FileDescriptor;

pub struct BtrfsDedup;

impl Deduper for BtrfsDedup {
    fn dedup_groups(&mut self, groups: Vec<DedupGroup>, state: &'static RwLock<State>, opts: &'static Opts) -> AnyhowResult<()> {
        // The dedups are split in batches to fit the os cache for readahead
        // the available file cache is estimated by the unused os memory
        // all files in batch will be opened 

        let file_split_round = 1024*1024*12;

        let max_dups_per_group = 127; // TODO determine amount of files we can open currently

        let allow_range_split = false;

        let mut cache_info = CacheUsable::new(file_split_round*2 .. opts.dedup_budget);

        let mut cache_max = cache_info.get(); //only refresh after actual submit

        let mut s = state.write();

        let mut submit_buf = Vec::<(DedupGroup,bool)>::new();

        let mut groups = VecDeque::from(groups);

        while !groups.is_empty() || !submit_buf.is_empty() {
            // This inner "loop" is kinda like a checklist, if we have to change something on the bufs, we "start over" with continue; or break if we're good
            while !groups.is_empty() || !submit_buf.is_empty() {
                // Current submit usage
                let mut submit_sum = 0;
                let mut submit_usage = 0;
                
                for (f,_) in &submit_buf {
                    submit_usage += f.usage();
                    submit_sum += f.sum();
                }

                // Populate from input
                if let Some(g) = groups.front() {
                    // No empty groups
                    if g.dups.is_empty() {
                        groups.pop_front();
                        continue;
                    }
                    // We always need one on the submit buf
                    if submit_buf.is_empty() {
                        submit_buf.push((groups.pop_front().unwrap(),true));
                        continue;
                    }
                    // Fit more in if we can
                    if submit_sum + g.sum() <= max_dups_per_group && submit_usage + g.usage() <= cache_max {
                        submit_buf.push((groups.pop_front().unwrap(),true));
                        continue;
                    }
                }

                // If all fits, we can submit now
                if submit_sum <= max_dups_per_group && submit_usage <= cache_max {
                    break;
                }

                if submit_buf.is_empty() {
                    break;
                }
                assert!(submit_buf.len() == 1);

                // If too many files, split end and put back
                let max_group_files = (cache_max / submit_buf[0].0.range_len()).min(max_dups_per_group).max(2);
                if max_group_files < submit_sum {
                    let end = submit_buf[0].0.split_off_end_at_candidate_n(max_group_files as usize - 1);
                    groups.push_front(end);
                    continue;
                }

                assert!(max_group_files == 2 && submit_buf[0].0.sum() == 2);

                // Now we have to range-split that single dedup. This will only be done on a single group with only two files
                if allow_range_split {
                    let mut g = submit_buf.remove(0).0;

                    let max_range_size = cache_max / g.sum(); // group.sum == 2 at this point
                    let max_range_size = max_range_size/file_split_round*file_split_round;
                    let max_range_size = max_range_size.min(g.range_len());
                    assert!(max_range_size*g.sum() <= cache_max);
                    assert!(max_range_size*g.sum() < g.usage());
                    let max_range_size = max_range_size.max(file_split_round);

                    while !g.range.is_empty() {
                        let first_half = g.split_off_start_range(max_range_size);

                        let first_half_usage = first_half.usage();
                        let is_last_part = g.range.is_empty();

                        dedup_group_batch(&[(first_half,is_last_part)], &mut s, opts, first_half_usage)?;
                    }

                    break;
                }

                // We couldn't do anything, so let's just commit the single thing
                break;
            }

            if !submit_buf.is_empty() {
                dedup_group_batch(&*submit_buf, &mut s, opts, submit_buf.iter().map(|v| v.0.usage()).sum())?;
                submit_buf.clear();
            }

            cache_max = cache_info.get();
        }
        
        Ok(())
    }
}

pub fn dedup_group_batch(current: &[(DedupGroup,bool)], state: &mut State, opts: &'static Opts, batch_size: u64) -> AnyhowResult<()> {
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
        for (group,last_part) in current {
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
            if *last_part {
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
    'g: for (group,last_part) in current {
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

        opened.push((group,senpai_fd,dups_fd,*last_part));
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
        for &(_,fd,range) in &readahead {
            unsafe{
                libc::posix_fadvise(
                    fd.get_value(),
                    range.start as i64,
                    (range.end - range.start) as i64,
                    libc::POSIX_FADV_SEQUENTIAL,
                );
            }
        }
        for &(_,fd,range) in &readahead {
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

        fn dedup(group_range: &Range<u64>, senpai_fd: &FileDescriptor, dups_fd: &[FileDescriptor], real: bool) -> (Result<(),String>,DedupeRange) {
            let dest_infos: Vec<_> = dups_fd.iter()
            .map(|fd| DedupeRangeDestInfo {
                dest_fd: fd.get_value() as i64,
                dest_offset: group_range.start,
                bytes_deduped: 0,
                status: DedupeRangeStatus::Same,
            })
            .collect();

            let mut dedup_range = DedupeRange {
                src_offset: group_range.start,
                src_length: group_range.end - group_range.start,
                dest_infos,
            };

            if real {
                let result = deduplicate_range(
                    senpai_fd.get_value(),
                    &mut dedup_range,
                );
    
                (result,dedup_range)
            } else {
                (Ok(()),dedup_range)
            }
        }

        let (result,dedup_range) = dedup(&group.range,&senpai_fd,&dups_fd,real);

        let mut deduped = 0;

        if let Err(e) = result {
            dprintln!("\tError deduplicating: {}",e);

            for (f,&id) in dups_fd.iter().zip(group.dups.iter()) {
                let (result,dedup_range) = dedup(&group.range,&senpai_fd,std::slice::from_ref(f),real);

                deduped += dedup_range.dest_infos[0].bytes_deduped;

                if dedup_range.dest_infos[0].status == DedupeRangeStatus::Differs {
                    let path = &state.tree[id].path;
                    dprintln!("\t\tNot deduped {}",opts.path_disp(path));
                } else {
                    state.tree[id].dedup_state = Some(true);
                }
            }
        }

        DISP_PROCESSED_BYTES.fetch_add(group.dups.len() as u64 * (group.range.end - group.range.start),Ordering::Relaxed);
        if last_part {
            DISP_PROCESSED_FILES.fetch_add(group.dups.len() as u64,Ordering::Relaxed);
        }

        for (i,&id) in dedup_range.dest_infos.iter().zip(group.dups.iter()) {
            deduped += i.bytes_deduped;
            
            if i.status == DedupeRangeStatus::Differs {
                let path = &state.tree[id].path;
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
