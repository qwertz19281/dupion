use super::*;
use std::{sync::{atomic::Ordering, Arc}};
use util::{disp_relevant_files, Hash, disp_relevant_bytes, Size};

use state::State;

/// Stored path MUST be canonical
#[derive(Clone)]
pub struct VfsEntry {
    pub path: Arc<Path>,
    pub ctime: Option<i64>,
    pub file_size: Option<Size>,
    pub dir_size: Option<Size>,
    pub file_hash: Option<Hash>,
    pub dir_hash: Option<Hash>,
    pub childs: Vec<VfsId>,
    pub valid: bool,
    pub is_file: bool,
    pub is_dir: bool,
    pub(super) was_file: bool,
    pub(super) was_dir: bool,
    pub file_shadowed: bool,
    pub dir_shadowed: bool,
    pub unique: bool,
    pub disp_relevated: bool,
}

impl VfsEntry {
    pub fn new(path: Arc<Path>) -> Self {
        Self{
            path,
            ctime: None,
            file_size: None,
            dir_size: None,
            file_hash: None,
            dir_hash: None,
            childs: Vec::new(),
            valid: false,
            is_file: false,
            is_dir: false,
            was_file: false,
            was_dir: false,
            file_shadowed: false,
            dir_shadowed: false,
            unique: false,
            disp_relevated: false,
        }
    }

    pub fn disp_add_relevant(&mut self) {
        if !self.disp_relevated && self.file_hash.is_none() {
            let size = self.file_size.unwrap() as usize;
            disp_relevant_bytes.fetch_add(size as usize,Ordering::Relaxed);
            disp_relevant_files.fetch_add(1,Ordering::Relaxed);
            self.disp_relevated = true;
        }
    }

    pub fn shadowed(&self, t: VfsEntryType) -> bool {
        match t {
            VfsEntryType::File => self.file_shadowed,
            VfsEntryType::Dir => self.dir_shadowed,
        }
    }
    pub fn size(&self, t: VfsEntryType) -> Option<Size> {
        match t {
            VfsEntryType::File => self.file_size,
            VfsEntryType::Dir => self.dir_size,
        }
    }
    pub fn is2(&self, t: VfsEntryType) -> bool {
        match t {
            VfsEntryType::File => self.is_file,
            VfsEntryType::Dir => self.is_dir,
        }
    }

    pub fn file_or_dir_props(&self) -> (Option<Size>,Option<Hash>) {
        //let e = self.path.to_string_lossy().as_ref();
        //assert_eq!(self.dir_size.is_some(),self.dir_hash.is_some());
        //assert_eq!(self.file_size.is_some(),self.file_hash.is_some());
        if self.file_size.is_some() {
            self.file_props()
        }else{
            self.dir_props()
        }
    }
    pub fn file_props(&self) -> (Option<Size>,Option<Hash>) {
        (self.file_size.clone(),self.file_hash.clone())
    }
    pub fn dir_props(&self) -> (Option<Size>,Option<Hash>) {
        (self.dir_size.clone(),self.dir_hash.clone())
    }

    pub fn exists(&self) -> bool {
        self.is_file || self.is_dir
    }
}

#[derive(PartialEq,Clone,Copy,Debug)]
pub enum VfsEntryType {
    File,
    Dir,
}

impl VfsEntryType {
    pub fn order(&self) -> u8 {
        match self {
            VfsEntryType::File => 2,
            VfsEntryType::Dir => 1,
        }
    }
}

impl Vfs {
    pub fn for_recursive(&mut self, id: VfsId, with_root: bool, f: fn(&mut VfsEntry)) {
        if with_root {
            f(&mut self[id]);
        }
        for c in self[id].childs.clone() {
            assert!(c != id);
            self.for_recursive(c,true,f);
        }
    }
}

impl State {
    pub fn validate(&mut self, id: VfsId, ctime: i64, check_file_size: Option<u64>, check_dir_size: Option<u64>) -> bool {
        let mut valid = self.tree[id].ctime == Some(ctime);
        if let Some(s) = check_file_size {
            valid &= self.tree[id].file_size == Some(s);
        }
        if let Some(s) = check_dir_size {
            valid &= self.tree[id].dir_size == Some(s);
        }
        if valid { //TODO also validate size
            //valid, recursive validate
            self.for_recursive2(id, true, |s,id| {
                s.tree[id].valid = true;
                s.tree[id].is_file |= s.tree[id].was_file;
                s.tree[id].is_dir |= s.tree[id].was_dir;

                if s.tree[id].is_file {
                    if s.tree[id].file_size.is_some() {
                        s.push_to_size_group(id,true,false).unwrap();
                    }
                    if s.tree[id].file_hash.is_some() {
                        s.push_to_hash_group(id,true,false).unwrap();
                    }
                }
            });
            true
        }else{
            let s = &mut self.tree[id];
            //eprintln!("MISS {}",s.path.to_string_lossy());
            s.file_size = None;
            s.file_hash = None;
            s.dir_size = None;
            s.dir_hash = None;
            s.ctime = Some(ctime);
            s.valid = true;
            false
        }
    }
    
    pub fn for_recursive2(&mut self, id: VfsId, with_root: bool, f: fn(&mut Self,VfsId)) {
        if with_root {
            f(self,id);
        }
        for c in self.tree[id].childs.clone() {
            assert!(c != id);
            self.for_recursive2(c,true,f);
        }
    }
}