//   platter-walk
//   Copyright (C) 2017 The 8472
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use btrfs::{get_file_extent_map_noloop, linux::{get_file_extent_map_for_path_noloop, FileExtent}, FileDescriptor};
use rustc_hash::FxHashMap;
use std::fs::*;
use std::os::unix::fs::DirEntryExt;
use std::path::PathBuf;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::path::Path;
use std::os::unix::fs::MetadataExt;
use std::os::unix::io::AsRawFd;
use std::os::unix::io::FromRawFd;
use std::cmp::Reverse;

pub struct Entry<D> where D: Default {
    path: PathBuf,
    ftype: FileType,
    ino: u64,
    pub metadata: Option<std::fs::Metadata>,
    pub canon_path: Option<PathBuf>,
    extents: Vec<FileExtent>,
    pub data: D,
}

impl<D> Entry<D> where D: Default {
    pub fn new(buf: PathBuf, ft: FileType, ino: u64, extents: Vec<FileExtent>, data: D) -> Self {
        Entry {
            path: buf,
            ftype: ft,
            ino,
            metadata: None,
            canon_path: None,
            extents,
            data,
        }
    }

    pub fn ino(&self) -> u64 {
        self.ino
    }

    pub fn file_type(&self) -> FileType {
        self.ftype
    }

    pub fn path(&self) -> &Path {
        self.path.as_path()
    }

    fn extent_sum(&self) -> u64 {
        self.extents.iter().map(|e| e.length).sum()
    }

    pub fn extents(&self) -> impl Iterator<Item=&FileExtent> {
        self.extents.iter()
    }
}

impl<D> PartialEq for Entry<D> where D: Default {
    fn eq(&self, other: &Self) -> bool {
        return self.path.eq(&other.path)
    }
}

impl<D> PartialEq<Path> for Entry<D> where D: Default {
    fn eq(&self, p: &Path) -> bool {
        return self.path.eq(p)
    }
}

pub struct ToScan<D> where D: Default {
    phy_sorted : BTreeMap<u64, Entry<D>>,
    phy_sorted_leaves: Vec<(u64, Entry<D>)>,
    unordered : VecDeque<Entry<D>>,
    cursor: u64,
    current_dir: Option<ReadDir>,
    inode_ordered: Vec<Entry<D>>,
    prefilter: Option<Box<dyn Fn(&Path, &FileType,&mut D) -> bool>>,
    phase: Phase,
    order: Order,
    batch_size: usize,
    prefetched: FxHashMap<PathBuf, u64>,
    mountpoints: Vec<mnt::MountEntry>,
    prefetch_cap: usize
}

#[derive(PartialEq, Copy, Clone)]
pub enum Order {
    /// Return directory entries sorted by physical offset of the file contents
    /// Can be used to get sequential reads over multiple files
    Content
}

#[derive(PartialEq)]
enum Phase {
    DirWalk,
    InodePass,
    ContentPass
}


use Order::*;

impl<D> ToScan<D> where D: Default {

    pub fn new() -> Self {
        ToScan {
            phy_sorted: BTreeMap::new(),
            phy_sorted_leaves: vec![],
            unordered: VecDeque::new(),
            cursor: 0,
            current_dir: None,
            inode_ordered: vec![],
            order: Content,
            phase: Phase::DirWalk,
            batch_size: 1024,
            prefilter: None,
            prefetched: FxHashMap::default(),
            mountpoints: vec![],
            prefetch_cap: 0
        }
    }

    pub fn set_order(&mut self, ord: Order) -> &mut Self {
        self.order = ord;
        self
    }

    pub fn prefetch_dirs(&mut self, val: bool) {
        if !val {
            self.mountpoints = vec![];
            return;
        }

        self.mountpoints = match mnt::MountIter::new_from_proc() {
            Ok(m) => m,
            Err(_) => {
                self.mountpoints = vec![];
                return
            }
        }.filter_map(|e| e.ok()).collect();
    }

    pub fn set_prefilter(&mut self, filter: Box<dyn Fn(&Path, &FileType, &mut D) -> bool>) {
        self.prefilter = Some(filter)
    }

    pub fn set_batchsize(&mut self, batch: usize) {
        self.batch_size = batch;
    }

    fn is_empty(&self) -> bool {
        self.phy_sorted.is_empty() && self.unordered.is_empty() && self.current_dir.is_none()
    }

    pub fn add_root(&mut self, path : PathBuf) -> std::io::Result<()> {
        let meta = std::fs::metadata(&path)?;
        self.add(Entry{path: path, ino: meta.ino(), metadata: None, canon_path: None, ftype: meta.file_type(), extents: vec![], data: D::default()}, None);
        Ok(())
    }

    fn get_next(&mut self) -> Option<Entry<D>> {
        self.prefetch();

        if !self.unordered.is_empty() {
            let res = self.unordered.pop_front();
            self.remove_prefetch(&res);
            return res;
        }

        let next_key = self.phy_sorted.range(self.cursor..).next().map(|(k,_)| *k);
        if let Some(k) = next_key {
            self.cursor = k;
            let res = self.phy_sorted.remove(&k);
            self.remove_prefetch(&res);
            return res;
        }

        None
    }

    fn remove_prefetch(&mut self, e : &Option<Entry<D>>) {
        if let &Some(ref e) = e {
            if let Some(_) = self.prefetched.remove(e.path()) {
                self.prefetch_cap = std::cmp::min(2048,self.prefetch_cap * 2 + 1);
            } else {
                self.prefetch_cap = 2;
                self.prefetched.clear();
            }

        }
    }

    fn prefetch(&mut self) {
        if self.mountpoints.is_empty() {
            return;
        }

        const LIMIT : u64 = 8*1024*1024;

        let consumed = self.prefetched.iter().map(|ref tuple| tuple.1).sum::<u64>();
        let mut remaining = LIMIT.saturating_sub(consumed);
        let prev_fetched = self.prefetched.len();

        // hysteresis
        if remaining < LIMIT/2 {
            return;
        }

        let unordered_iter = self.unordered.iter();
        let ordered_iter_front = self.phy_sorted.range(self.cursor..).map(|(_,v)| v);
        let ordered_iter_tail = self.phy_sorted.range(..self.cursor).map(|(_,v)| v);

        let mut prune = vec![];

        {
            let mut device_groups = FxHashMap::default();

            for e in unordered_iter.chain(ordered_iter_front).chain(ordered_iter_tail) {
                if remaining == 0 {
                    break;
                }

                if self.prefetched.len() > self.prefetch_cap + 1 {
                    break;
                }

                if self.prefetched.contains_key(e.path()) {
                    continue;
                }

                let size = e.extent_sum();
                remaining = remaining.saturating_sub(size);
                self.prefetched.insert(e.path().to_owned(), size);

                let mount = self.mountpoints.iter().rev().find(|mnt| e.path().starts_with(&mnt.file));

                // TODO: only try to open devices once
                match mount {
                    Some(&mnt::MountEntry {ref spec, ref vfstype, ..})
                    if vfstype == "ext4" || vfstype == "ext3"// || vfstype == "btrfs"
                    => {
                        let mount_slot = device_groups.entry(spec).or_insert(vec![]);
                        mount_slot.extend(&e.extents);
                    }
                    _ => {}
                }
            }

            for (p, extents) in device_groups {
                let mut ordered_extents = extents.to_vec();
                ordered_extents.sort_by_key(|e| e.physical);

                if let Ok(f) = File::open(p) {

                    let mut i = 0;

                    while i < ordered_extents.len() {
                        let ext1 = ordered_extents[i];
                        let offset = ext1.physical;
                        let mut end = offset + ext1.length;

                        for j in i+1..ordered_extents.len() {
                            let ref ext2 = ordered_extents[j];
                            if ext2.physical > end {
                                break;
                            }

                            i = j;

                            end = ext2.physical+ext2.length;
                        }

                        i+=1;

                        unsafe {
                            libc::posix_fadvise(f.as_raw_fd(), offset as i64, (end - offset) as i64, libc::POSIX_FADV_WILLNEED);
                        }
                    }
                } else {
                    prune.push(p.to_owned());
                }
            }

        }

        //println!("bytes: {} -> {}, f: {}->{}, sc: {}", LIMIT-consumed, remaining, prev_fetched ,self.prefetched.len(), self.prefetch_cap);

        if prune.len() > 0 {
            self.mountpoints.retain(|e| prune.contains(&e.spec));
        }


    }

    pub fn add(&mut self, to_add: Entry<D>, pos: Option<u64>) {
        match pos {
            Some(idx) => {
                if let Some(old) = self.phy_sorted.insert(idx, to_add) {
                    self.unordered.push_back(old);
                }
            }
            None => {
                self.unordered.push_back(to_add);
            }
        }
    }


}

impl<D> Iterator for ToScan<D> where D: Default {
    type Item = std::io::Result<Vec<(u64,Entry<D>)>>;

    //platter-wank is a hack for specialized manual content-sorting
    fn next(&mut self) -> Option<std::io::Result<Vec<(u64,Entry<D>)>>> {

        while self.phase == Phase::DirWalk && !self.is_empty() {
            if self.current_dir.is_none() {
                let nxt = match self.get_next() {
                    Some(e) => e,
                    None => {
                        self.cursor = 0;
                        continue;
                    }
                };

                match read_dir(nxt.path()) {
                    Ok(dir_iter) => {
                        self.current_dir = Some(dir_iter);
                    },
                    Err(open_err) => return Some(Err(open_err))
                }
            }

            let mut entry = None;

            if let Some(ref mut iter) = self.current_dir {
                entry = iter.next();
            }

            match entry {
                None => {
                    self.current_dir = None;
                    continue;
                }
                Some(Err(e)) => return Some(Err(e)),
                Some(Ok(dent)) => {
                    let meta = match dent.file_type() {
                        Ok(ft) => ft,
                        Err(e) => return Some(Err(e))
                    };

                    // TODO: Better phase-switching?
                    // move to inode pass? won't start the next dir before this one is done anyway
                    if meta.is_dir() {

                        let extents = get_file_extent_map_for_path_noloop(dent.path())
                            .unwrap_or_else(|_| Vec::new() );

                        let to_add = Entry::new(dent.path(), meta, dent.ino(), extents, D::default());

                        if !to_add.extents.is_empty() {
                            let offset = to_add.extents[0].physical;
                            self.add(to_add, Some(offset));
                        } else {
                            // TODO: fall back to inode-order? depth-first?
                            // skip adding non-directories in content order?
                            self.add(to_add, None);
                        }
                    }

                    let mut userdata = D::default();
                    
                    if let Some(ref filter) = self.prefilter {
                        if !filter(&dent.path(), &meta, &mut userdata) {
                            continue;
                        }
                    }

                    match self.order {
                        Order::Content => {
                            self.inode_ordered.push(Entry::new(dent.path(), meta, dent.ino(), vec![], userdata));
                        }
                    }
                }
            }

            if self.inode_ordered.len() >= self.batch_size {
                self.phase = Phase::InodePass;
                // reverse sort so we can pop
                self.inode_ordered.sort_by_key(|dent| Reverse(dent.ino()));
            }
        }


        if self.phase == Phase::InodePass || (self.is_empty() && self.inode_ordered.len() > 0)  {
            assert!(self.inode_ordered.len() > 0);

            match self.order {
                Order::Content => {
                    for mut e in self.inode_ordered.drain(..).rev() {
                        let (meta,extents) = file_meta_and_extents(e.path());
                        let offset = match extents {
                            Ok(ref extents) if !extents.is_empty() => extents[0].physical,
                            _ => 0
                        };
                        //The metadata should now be cached by the OS, so file size read shouldn't be slow
                        if e.ftype.is_file() {
                            if let Ok(meta) = meta {
                                e.metadata = Some(meta);
                            }
                            /*if let Ok(canon) = std::fs::canonicalize(e.path()) {
                                assert_eq!(canon,e.path());
                                e.canon_path = Some(canon);
                            }*/
                            assert!(e.path().is_absolute());
                            e.canon_path = Some(e.path().to_owned());
                        }
                        self.phy_sorted_leaves.push((offset, e));
                    }
                    self.phy_sorted_leaves.sort_by_key(|pair| pair.0);
                    self.phase = Phase::ContentPass;
                    assert!(self.phy_sorted_leaves.len() > 0);
                },
                _ => panic!("illegal state")
            }

        }

        if self.phase == Phase::ContentPass || (self.is_empty() && self.phy_sorted_leaves.len() > 0) {
            assert!(self.phy_sorted_leaves.len() > 0);
            let entries = std::mem::take(&mut self.phy_sorted_leaves);
            self.phy_sorted_leaves.reserve(entries.len());
            self.phase = Phase::DirWalk;
            return Some(Ok(entries))
        }

        None
    }

}

pub fn file_meta_and_extents(path: impl AsRef<Path>) -> (Result<Metadata,String>,Result<Vec<FileExtent>,String>) {

    let fd = FileDescriptor::open(
		&path,
		libc::O_RDONLY,
    );
    
    let fd = match fd {
        Ok(v) => v,
        Err(e) => {
            let s = format!("{}",e);
            return (Err(s.clone()),Err(s));
        }
    };

    let extents = get_file_extent_map_noloop (
        fd.get_value());
    
    let file = unsafe{File::from_raw_fd(fd.get_value())};
    let meta = file.metadata().map_err(|e| format!("{}",e) );
    std::mem::forget(file);
    
    (meta,extents)
}
