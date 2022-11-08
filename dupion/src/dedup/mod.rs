use super::*;
use parking_lot::RwLock;
use state::State;
use opts::Opts;
use util::*;
use vfs::{entry::VfsEntryType, VfsId};
use std::{sync::atomic::Ordering, ops::Range};

pub mod btrfs;
pub mod fd;

pub trait Deduper {
    fn dedup(&mut self, state: &'static RwLock<State>, opts: &'static Opts) -> AnyhowResult<()> {
        DISP_PROCESSED_FILES.store(0,Ordering::Relaxed);
        DISP_PREV.store(0,Ordering::Relaxed);
        DISP_PROCESSED_BYTES.store(0,Ordering::Relaxed);
        DISP_RELEVANT_FILES.store(0,Ordering::Relaxed);
        DISP_RELEVANT_BYTES.store(0,Ordering::Relaxed);
        DISP_DEDUPED_BYTES.store(0, Ordering::Relaxed);
        
        let s = state.write();
        let mut dest: Vec<DedupGroup> = Vec::with_capacity(s.hashes.len());

        for e in s.hashes.values() {
            if e.size == 0 {continue;} //TODO proper min size option

            let mut candidates: Vec<VfsId> = e.entries.iter()
                .filter(|&&(typ,id)| typ == VfsEntryType::File && s.tree[id].phys.is_some() )
                .map(|&(_,id)| id )
                .collect();

            if candidates.len() < 2 {continue;}

            let avg_phys = candidates.iter()
                .map(|&c| s.tree[c].phys.unwrap() )
                .sum::<u64>() / (candidates.len() as u64);

            let senpai = {
                let (idx,&new) = candidates.iter()
                    .enumerate()
                    .min_by_key(|(_,&id)| distance(avg_phys, s.tree[id].phys.unwrap()) )
                    .unwrap();

                candidates.remove(idx);

                new
            };

            candidates.retain(|&id|
                id != senpai &&
                (opts.aggressive_dedup || s.tree[id].phys.unwrap() != s.tree[senpai].phys.unwrap())
            );
            if candidates.is_empty() {continue;}
            candidates.sort_by_key(|&id| s.tree[id].phys.unwrap() );

            let size = s.tree[senpai].file_size.unwrap();

            DISP_RELEVANT_BYTES.fetch_add(candidates.len() as u64*size,Ordering::Relaxed);
            DISP_RELEVANT_FILES.fetch_add(candidates.len() as u64,Ordering::Relaxed);

            while candidates.len() > 127 { //TODO move to specific dedup handler
                let remainder = candidates.split_off(127);

                dest.push(DedupGroup{
                    sum: candidates.len() as u64 +1,
                    senpai,
                    dups: candidates,
                    range: 0..size,
                    file_size: size,
                    avg_phys,
                });

                candidates = remainder;
            }

            dest.push(DedupGroup{
                sum: candidates.len() as u64 +1,
                senpai,
                dups: candidates,
                range: 0..size,
                file_size: size,
                avg_phys,
            });
        }

        drop(s);

        dest.sort_by_key(|g| g.avg_phys );
        dest.shrink_to_fit();

        self.dedup_groups(dest, state, opts)?;

        Ok(())
    }

    fn dedup_groups(&mut self, groups: Vec<DedupGroup>, state: &'static RwLock<State>, opts: &'static Opts) -> AnyhowResult<()>;
}

#[derive(Clone)]
pub struct DedupGroup {
    pub senpai: VfsId,
    pub dups: Vec<VfsId>,
    pub range: Range<u64>,
    pub avg_phys: u64,
    pub file_size: u64,
    pub sum: u64,
}

fn distance(a: u64, b: u64) -> u64 {
    a.max(b) - a.min(b)
}
