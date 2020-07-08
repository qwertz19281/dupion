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
        disp_processed_files.store(0,Ordering::Release);
        disp_prev.store(0,Ordering::Release);
        disp_processed_bytes.store(0,Ordering::Release);
        disp_relevant_files.store(0,Ordering::Release);
        disp_relevant_bytes.store(0,Ordering::Release);
        disp_deduped_bytes.store(0, Ordering::Release);
        
        let s = state.write();
        let mut dest: Vec<DedupGroup> = Vec::with_capacity(s.hashes.len());

        for e in s.hashes.values() {
            if e.typ != VfsEntryType::File {continue;}
            if e.size == 0 {continue;} //TODO proper min size option

            let mut senpai: Option<VfsId> = e.entries.iter()
                .find(|&&id| s.tree[id].phys.is_some() && s.tree[id].dedup_state == Some(true) )
                .cloned();

            let mut candidates: Vec<VfsId> = e.entries.iter()
                .filter(|&&id| s.tree[id].phys.is_some() && s.tree[id].dedup_state.is_none() )
                .cloned()
                .collect();

            if candidates.is_empty() {continue;}

            let mut cand_avg_phys = 0;
            let mut avg_phys = 0;

            for &c in &candidates {
                cand_avg_phys += s.tree[c].phys.unwrap();
            }
            avg_phys = cand_avg_phys;
            cand_avg_phys /= candidates.len() as u64;

            if senpai.is_none() {
                if candidates.len() < 2 {continue;}

                let (idx,new) = candidates.iter()
                    .enumerate()
                    .min_by_key(|(_,id)| distance(cand_avg_phys, s.tree[**id].phys.unwrap()) )
                    .unwrap();

                senpai = Some(*new);

                candidates.remove(idx);
            }

            let senpai = senpai.unwrap();

            candidates.retain(|&id| s.tree[id].phys.unwrap() != s.tree[senpai].phys.unwrap() );
            if candidates.is_empty() {continue;}
            candidates.sort_by_key(|&id| s.tree[id].phys.unwrap() );
            candidates.truncate(512); //TODO real max open file
            candidates.shrink_to_fit();
            
            avg_phys += s.tree[senpai].phys.unwrap();
            avg_phys /= candidates.len() as u64 +1;

            let size = s.tree[senpai].file_size.unwrap();

            disp_relevant_bytes.fetch_add(candidates.len()*size as usize,Ordering::Relaxed);
            disp_relevant_files.fetch_add(candidates.len(),Ordering::Relaxed);

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
