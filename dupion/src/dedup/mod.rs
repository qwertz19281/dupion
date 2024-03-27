use super::*;
use parking_lot::RwLock;
use state::State;
use opts::Opts;
use util::*;
use vfs::{entry::VfsEntryType, VfsId};
use std::cmp::Reverse;
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
        let mut candidates = Vec::with_capacity(1024);

        for e in s.hashes.values() {
            if e.size == 0 {continue;} //TODO proper min size option

            candidates.clear();

            candidates.extend(
            e.entries.iter()
                    .filter(|&&(typ,id)| 
                        typ == VfsEntryType::File
                        && s.tree[id].phys.is_some()
                        && s.tree[id].phys != Some(0)
                        && s.tree[id].valid
                        && s.tree[id].n_extents.is_some()
                    )
                    .map(|&(_,id)| DedupCandidate {
                        id,
                        phys: s.tree[id].phys.unwrap(),
                        phys_occurrences: 0,
                        file_size: s.tree[id].file_size.unwrap(),
                        n_extends: s.tree[id].n_extents.unwrap(),
                        ctime: s.tree[id].ctime.unwrap()
                    })
            );

            if candidates.len() < 2 {continue;}

            let avg_phys = candidates.iter()
                .map(|c| c.phys )
                .sum::<u64>() / (candidates.len() as u64);
            
            candidates.sort_by_key(|c| c.phys );

            count_phys_occurrences_sorted(&mut candidates);

            let senpai = {
                let (idx,&new) = candidates.iter()
                    .enumerate()
                    .min_by_key(|(_,c)| (
                        // senpai prioritization of candidate with the:
                        // 1. least extents
                        c.n_extends,
                        // 2. most common phys in group
                        Reverse(c.phys_occurrences),
                        // 3. oldest ctime
                        c.ctime,
                        // 4. smallest distance from avg phys
                        distance(avg_phys, c.phys)
                    ))
                    .unwrap();

                candidates.remove(idx);

                new
            };

            candidates.retain(|c|
                c.id != senpai.id &&
                (opts.aggressive_dedup || c.phys != senpai.phys)
            );
            if candidates.is_empty() {continue;}

            let size = senpai.file_size;

            DISP_RELEVANT_BYTES.fetch_add(candidates.len() as u64*size,Ordering::Relaxed);
            DISP_RELEVANT_FILES.fetch_add(candidates.len() as u64,Ordering::Relaxed);

            dest.push(DedupGroup{
                senpai: senpai.id,
                dups: candidates.iter().map(|c| c.id ).collect(),
                range: 0..size,
                actual_file_size: size,
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

#[derive(Clone, Copy)]
pub struct DedupCandidate {
    pub id: VfsId,
    pub phys: u64,
    pub phys_occurrences: usize,
    pub file_size: u64,
    pub n_extends: usize,
    pub ctime: i64,
}

fn count_phys_occurrences_sorted(v: &mut [DedupCandidate]) {
    if v.is_empty() {return;}

    let mut current_phys = v[0].phys;
    let mut current_count = 0;

    for v in &mut *v {
        if current_phys != v.phys {
            current_phys = v.phys;
            current_count = 0;
        }

        current_count += 1;

        v.phys_occurrences = current_count;
    }

    for v in v.iter_mut().rev() {
        if current_phys != v.phys {
            current_phys = v.phys;
            current_count = v.phys_occurrences;
        }

        v.phys_occurrences = current_count;
    }
}

#[derive(Clone)]
pub struct DedupGroup {
    pub senpai: VfsId,
    pub dups: Vec<VfsId>,
    pub range: Range<u64>,
    pub avg_phys: u64,
    pub actual_file_size: u64,
}

impl DedupGroup {
    pub fn sum(&self) -> u64 {
        self.dups.len() as u64 + 1
    }

    pub fn range_len(&self) -> u64 {
        debug_assert!(self.range.end >= self.range.start);
        self.range.end - self.range.start.min(self.range.end)
    }

    pub fn usage(&self) -> u64 {
        self.range_len() * self.sum()
    }

    /// return the first half and keep last half in &mut self
    pub fn split_off_start_at_candidate_n(&mut self, at: usize) -> Self {
        let dups_remainder = self.dups.split_off(at);
        
        let dups = std::mem::replace(&mut self.dups, dups_remainder);

        Self {
            senpai: self.senpai,
            dups,
            range: self.range.clone(),
            avg_phys: self.avg_phys, //TODO recalculate avg_phys
            actual_file_size: self.actual_file_size,
        }
    }

    /// return the last half and keep first half in &mut self
    pub fn split_off_end_at_candidate_n(&mut self, at: usize) -> Self {
        let dups = self.dups.split_off(at);

        Self {
            senpai: self.senpai,
            dups,
            range: self.range.clone(),
            avg_phys: self.avg_phys, //TODO recalculate avg_phys
            actual_file_size: self.actual_file_size,
        }
    }

    /// return the first half and keep last half in &mut self
    pub fn split_off_start_range(&mut self, len: u64) -> Self {
        let len = len.min(self.range_len());
        let first_range = self.range.start .. self.range.start + len;
        let last_range = self.range.start + len .. self.range.end;

        let mut first_part = self.clone();

        first_part.range = first_range;
        self.range = last_range;

        first_part
    }
}

fn distance(a: u64, b: u64) -> u64 {
    a.max(b) - a.min(b)
}
