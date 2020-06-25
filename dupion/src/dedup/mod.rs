use super::*;
use parking_lot::RwLock;
use state::State;
use opts::Opts;
use util::Hash;
use vfs::{entry::VfsEntryType, VfsId};
use std::ops::Range;

pub trait Deduper {
    fn dedup(&mut self, state: &'static RwLock<State>, opts: &'static Opts) -> AnyhowResult<()> {
        let state = state.write();
        let mut dest: Vec<DedupGroup> = Vec::with_capacity(state.hashes.len());

        for e in state.hashes.values() {
            if e.typ != VfsEntryType::File {continue;}
            let mut senpai: Option<VfsId> = e.entries.iter()
                .find(|&&id| state.tree[id].phys.is_some() && state.tree[id].dedup_state == Some(true) )
                .cloned();

            let mut candidates: Vec<VfsId> = e.entries.iter()
                .filter(|&&id| state.tree[id].phys.is_some() && state.tree[id].dedup_state.is_none() )
                .cloned()
                .collect();

            if candidates.is_empty() {continue;}

            let mut cand_avg_phys = 0;
            let mut avg_phys = 0;

            for &c in &candidates {
                cand_avg_phys += state.tree[c].phys.unwrap();
            }
            avg_phys = cand_avg_phys;
            cand_avg_phys /= candidates.len() as u64;

            if senpai.is_none() {
                if candidates.len() < 2 {continue;}

                let (idx,new) = candidates.iter()
                    .enumerate()
                    .min_by_key(|(_,id)| distance(cand_avg_phys, state.tree[**id].phys.unwrap()) )
                    .unwrap();

                senpai = Some(*new);

                candidates.remove(idx);
            }

            let senpai = senpai.unwrap();
            
            avg_phys += state.tree[senpai].phys.unwrap();
            avg_phys /= candidates.len() as u64 +1;

            dest.push(DedupGroup{
                senpai,
                dups: candidates,
                range: todo!(),
                avg_phys,
            });
        }

        Ok(())
    }

    fn dedup_groups(&mut self, groups: &[DedupGroup], state: &'static RwLock<State>, opts: &'static Opts) -> AnyhowResult<()>;
}

pub struct DedupGroup {
    pub senpai: VfsId,
    pub dups: Vec<VfsId>,
    pub range: Range<u64>,
    pub avg_phys: u64,
}

fn distance(a: u64, b: u64) -> u64 {
    a.max(b) - a.min(b)
}
