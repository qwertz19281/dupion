//   reapfrog
//   Copyright (C) 2017 The 8472
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

extern crate libc;

use std::collections::VecDeque;
use std::fs::File;
use std::fs::Metadata;
use std::io::Read;
use std::os::unix::io::AsRawFd;
use std::path::Path;

const DROPBEHIND_BLOCK : u64 = 512 * 1024;
const PREFETCH_SHIFT : u8 = 16;
const PREFETCH_BLOCK : u64 = 1 << PREFETCH_SHIFT;
const MAX_OPEN : usize = 512;
const DEFAULT_BUDGET : u64 = 8*1024*1024;

struct Prefetch<U> {
    p: U,
    f: File,
    read_pos: u64,
    prefetch_pos: u64,
    to_drop: u64,
    length: u64,
    metadata: Metadata,
}

impl<U> Prefetch<U> {
    fn new(f: File, metadata: Metadata, len: u64, p: U) -> Self {
        unsafe {
            libc::posix_fadvise(f.as_raw_fd(), 0, 0, libc::POSIX_FADV_SEQUENTIAL);
        }
        Prefetch{f, read_pos: 0, length: len, p, to_drop: 0, prefetch_pos: 0, metadata}
    }
}

pub struct MultiFileReadahead<Src,U> {
    source: Src,
    open: VecDeque<Result<Prefetch<U>, std::io::Error>>,
    dropbehind: bool,
    budget: u64,
}


pub struct Reader<'a, T: 'a, U: 'a> {
    owner: &'a mut MultiFileReadahead<T,U>
}

impl<'a, T, U> Reader<'a, T, U> where T: Iterator<Item=U>, U: AsRef<Path> {

    pub fn metadata(&self) -> &Metadata {
        &self.owner.open[0].as_ref().expect("expect that readers are only created for successfully opened files").metadata
    }

    pub fn data(&self) -> &U {
        &self.owner.open[0].as_ref().expect("expect that readers are only created for successfully opened files").p
    }

}

impl<'a, T, U> Read for &'a mut Reader<'a, T, U>
    where T: Iterator<Item=U>, U: AsRef<Path>
{
    fn read(&mut self, buf: &mut [u8]) -> std::result::Result<usize, std::io::Error> {
        let result = {
            let drop = self.owner.dropbehind;
            let ref mut fetch = self.owner.open[0].as_mut().expect("expect that readers are only created for successfully opened files");
            let result = fetch.f.read(buf);
            if let Ok(bytes) = result {
                fetch.read_pos += bytes as u64;
                if drop {
                    fetch.to_drop += bytes as u64;
                    if fetch.to_drop >= DROPBEHIND_BLOCK {
                        unsafe {
                            let drop_offset = fetch.read_pos - fetch.to_drop;
                            libc::posix_fadvise(fetch.f.as_raw_fd(), drop_offset as i64, fetch.to_drop as i64, libc::POSIX_FADV_DONTNEED);
                        }
                        fetch.to_drop = 0;
                    }
                }
            }


            result
        };
        self.owner.advance();
        result
    }
}

impl<Src: Iterator<Item=U>,U> MultiFileReadahead<Src,U> where U: AsRef<Path> {

    pub fn new(src: Src) -> Self {
        MultiFileReadahead {source: src, open: VecDeque::new(), dropbehind: false, budget: DEFAULT_BUDGET}
    }

    pub fn dropbehind(&mut self, v : bool) {
        self.dropbehind = v;
    }

    pub fn budget(&mut self, v : u64) {
        self.budget = v;
    }

    fn advance(&mut self) {

        let consumed = self.open.iter().map(|o| {
            match *o {
                Ok(ref o) => o.prefetch_pos.saturating_sub(o.read_pos),
                Err(_) => 0
            }
        }).sum::<u64>();

        // we may overshoot our budget slightly, saturate to zero
        let mut budget = self.budget.saturating_sub(consumed);

        // hysteresis: let the loop expend the budget to ~100% if possible, then don't loop until we fall to 50%
        if budget < consumed {
            return
        }

        for i in 0.. {
            if budget < PREFETCH_BLOCK { break; }

            if i == self.open.len() && !self.add_file() {
                break
            }

            if i > MAX_OPEN { break }

            let ref mut p = match self.open[i] {
                Ok(ref mut p) => p,
                Err(_) => continue
            };

            let old_pos = std::cmp::max(p.read_pos, p.prefetch_pos);
            if old_pos >= p.length { continue; }
            // round down
            let internal_budget = (budget >> PREFETCH_SHIFT) << PREFETCH_SHIFT;
            let mut prefetch_length = std::cmp::min(p.length - old_pos, internal_budget);
            let mut new_pos = old_pos + prefetch_length;
            // round up to multiple so that readaheads are aligned
            // allows slight overshoot of budget
            new_pos = (new_pos + PREFETCH_BLOCK - 1) & !(PREFETCH_BLOCK - 1);
            new_pos = std::cmp::min(p.length, new_pos);

            prefetch_length = new_pos - old_pos;

            unsafe {
                libc::posix_fadvise(p.f.as_raw_fd(), old_pos as i64, prefetch_length as i64, libc::POSIX_FADV_WILLNEED);
            }

            budget = budget.saturating_sub(prefetch_length);
            p.prefetch_pos = new_pos;
        }
    }

    fn add_file(&mut self) -> bool {
        let mut read = false;
        for _ in 0..MAX_OPEN/4 {
            match self.source.next() {
                None => break,
                Some(p) => {
                    let f = match File::open(&p) {
                        Ok(f) => f,
                        Err(e) => {
                            self.open.push_back(Err(e));
                            continue;
                        }
                    };

                    let meta = match f.metadata() {
                        Ok(m) => m,
                        Err(e) => {
                            self.open.push_back(Err(e));
                            continue;
                        }
                    };

                    let len = meta.len();

                    self.open.push_back(Ok(Prefetch::new(f, meta, len, p)));
                    read=true;
                }
            }
        }
        read
    }

    pub fn next(&mut self) -> Option<Result<Reader<Src,U>, std::io::Error>> {
        // discard most recent file
        if let Some(Ok(p)) = self.open.pop_front() {
            if p.to_drop > 0 {
                unsafe {
                    libc::posix_fadvise(p.f.as_raw_fd(), 0, 0, libc::POSIX_FADV_DONTNEED);
                }
            }
        }
        self.advance();

        if self.open.is_empty() && !self.add_file() {
            return None;
        };
        if self.open[0].is_err() {
            return Some(Err(self.open.pop_front().unwrap().err().unwrap()))
        }
        Some(Ok(Reader{owner: self}))
    }
}
