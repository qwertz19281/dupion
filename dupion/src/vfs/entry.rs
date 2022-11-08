use super::*;
use std::{sync::{atomic::Ordering, Arc}, ffi::OsString};
use util::{DISP_RELEVANT_FILES, Hash, DISP_RELEVANT_BYTES, Size};

use state::State;

/// Stored path MUST be canonical
#[derive(Clone)]
pub struct VfsEntry {
    pub path: Arc<Path>,
    pub plc: OsString,
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
    pub failure: Option<u64>,
    pub treediff_stat: u8,
    pub dedup_state: Option<bool>,
    pub phys: Option<u64>,
}

const _: () = assert!(std::mem::size_of::<VfsEntry>() == 176);

impl VfsEntry {
    pub fn new(path: Arc<Path>) -> Self {
        Self{
            plc: to_plc(&path),
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
            failure: None,
            treediff_stat: 0,
            dedup_state: None,
            phys: Some(0),
        }
    }

    pub fn disp_add_relevant(&mut self) {
        if !self.disp_relevated && self.file_hash.is_none() {
            let size = self.file_size.unwrap();
            DISP_RELEVANT_BYTES.fetch_add(size,Ordering::Relaxed);
            DISP_RELEVANT_FILES.fetch_add(1,Ordering::Relaxed);
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
        if self.is_file { //TODO FIX unverified change from self.file_size.is_some()
            self.file_props()
        }else{
            self.dir_props()
        }
    }
    pub fn file_props(&self) -> (Option<Size>,Option<Hash>) {
        (self.file_size,self.file_hash.clone())
    }
    pub fn dir_props(&self) -> (Option<Size>,Option<Hash>) {
        (self.dir_size,self.dir_hash.clone())
    }

    pub fn exists(&self) -> bool {
        self.is_file || self.is_dir
    }

    pub fn icon3(&self) -> char {
        match (self.is_dir,self.is_file) {
            (true,true) => 'A',
            (false,true) => 'F',
            (true,false) => 'D',
            (false,false) => 'X',
        }
    }
    pub fn icon_prio2(&self) -> u32 {
        match (self.is_dir,self.is_file) {
            (true,true) => 1,
            (false,true) => 2,
            (true,false) => 0,
            (false,false) => 3,
        }
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
            Self::File => 2,
            Self::Dir => 1,
        }
    }

    pub fn icon2(&self, is_dir: bool) -> char {
        match self {
            Self::File if is_dir => 'A',
            Self::File => 'F',
            Self::Dir => 'D',
        }
    }
    pub fn icon(&self) -> char {
        match self {
            Self::File => 'F',
            Self::Dir => 'D',
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
            self.set_valid(id)
        }else{
            let s = &mut self.tree[id];
            //dprintln!("MISS {}",s.path.to_string_lossy());
            s.file_size = None;
            s.file_hash = None;
            s.dir_size = None;
            s.dir_hash = None;
            s.dedup_state = None;
            s.phys = Some(0);
            s.ctime = Some(ctime);
            s.valid = true;
            false
        }
    }

    pub fn set_valid(&mut self, id: VfsId) -> bool {
        //assert!(self.tree[id].is_file);
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
    }

    /*pub fn clean_old_validations(&mut self, id: VfsId) -> bool {
        self.for_recursive2(id, true, |s,id| {
            s.tree[id].was_file = false;
            s.tree[id].was_dir = false;
        });
        true
    }*/
    
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
