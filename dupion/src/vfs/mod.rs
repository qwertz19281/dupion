use super::*;
use entry::VfsEntry;
use std::{ops::{IndexMut, Index, Deref}, path::{PathBuf, Path, Component}, sync::Arc, ffi::{OsStr, OsString}};

pub mod entry;
pub mod deser;

pub struct Vfs {
    pub entries: Vec<VfsEntry>,
    pub static_empty_arc_path: Arc<Path>,
}

impl Vfs {
    pub fn cid(&self, path: &Path) -> Option<VfsId> {
        is_absolute(path);
        let mut id = VfsId::ROOT;
        
        for c in path.components() {
            id = maybe!(self.child_of(id,&c));
        }

        assert_eq!(&*self[id].path,path);

        Some(id)
    }
    pub fn cid_and_create(&mut self, path: &Path) -> VfsId {
        let mut id = VfsId::ROOT;

        is_absolute(path);

        let mut current_build = PathBuf::with_capacity(path.as_os_str().len()*2);
        
        for c in path.components() {
            current_build.push(c);
            // at this point current_build is the new and id is the previous (parent)
            match self.child_of(id,&c) {
                Some(i) => {
                    self[id].is_dir = true;
                    id = i;
                },
                None => {
                    let e = VfsEntry::new(current_build.clone().into());
                    let new = self._insert_new_entry(e);
                    self[id].is_dir = true;
                    self[id].childs.push(new);
                    id = new;
                }
            }
        }

        assert_eq!(current_build,path);
        assert_eq!(&*self[id].path,path);

        id
    }
    pub fn resolve(&self, path: &Path) -> Option<&VfsEntry> {
        self.cid(path)
            .map(|id| &self[id] )
    }
    pub fn resolve_mut(&mut self, path: &Path) -> Option<&mut VfsEntry> {
        let id = self.cid(path);
        if let Some(id) = id {
            let e = &mut self[id];
            Some(e)
        }else{
            None
        }
    }
    pub fn resolve_or_create(&mut self, path: &Path) -> &mut VfsEntry {
        let id = self.cid_and_create(path);
        &mut self[id]
    }
    pub fn insert_new_entry(&mut self, e: VfsEntry) -> VfsId {
        let id = self.cid_and_create(&e.path);
        let ee = &mut self[id];
        *ee = e;
        id
    }
    pub fn _insert_new_entry(&mut self, e: VfsEntry) -> VfsId {
        let place = self.entries.len();
        self.entries.push(e);
        VfsId{evil_inner: place}
    }
    pub fn child_of(&self, id: VfsId, c: &Component) -> Option<VfsId> {
        assert_ne!(c,&Component::ParentDir);
        if c == &Component::CurDir {return Some(id);}

        let entry = &self[id];
        //is_absolute(&entry.path);
        for &cid in &entry.childs {
            let plc = &self[cid].plc;
            debug_assert_eq!(plc,&to_plc(&self[cid].path));
            if plc == c.as_os_str() {
                return Some(cid);
            }
        }
        None
    }
    /// Root MUST be dir
    pub fn new() -> Self {
        let static_empty_arc_path: Arc<Path> = PathBuf::new().into();
        let mut senf = Self{
            entries: Vec::with_capacity(65536),
            static_empty_arc_path: static_empty_arc_path.clone(),
        };
        senf.entries.push(VfsEntry{
            path: static_empty_arc_path,
            plc: OsString::with_capacity(0),
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
        });
        senf
    }
}

impl Index<VfsId> for Vfs {
    type Output = VfsEntry;
    fn index(&self, index: VfsId) -> &Self::Output {
        &self.entries[index.evil_inner]
    }
}
impl IndexMut<VfsId> for Vfs {
    fn index_mut(&mut self, index: VfsId) -> &mut Self::Output {
        &mut self.entries[index.evil_inner]
    }
}

#[macro_export]
macro_rules! maybe {
    ($oof:expr) => {
        match $oof {
            Some(f) => {
                f
            },
            None => {
                return None;
            },
        }
    };
}

#[derive(Copy,Clone,PartialEq,PartialOrd)]
pub struct VfsId {
    pub evil_inner: usize,
}

impl VfsId {
    pub const ROOT: VfsId = VfsId{evil_inner: 0};
}

pub fn is_absolute(path: &Path) {
    assert!(path.is_absolute() || {
        path.components().next().is_none()
    });
}

pub struct AbsPath<P> where P: AsRef<Path> {
    inner: P,
}

impl<P> From<P> for AbsPath<P> where P: AsRef<Path> {
    fn from(inner: P) -> Self {
        is_absolute(inner.as_ref());
        Self{inner}
    }
}

impl<P> Deref for AbsPath<P> where P: AsRef<Path> {
    type Target = P;
    fn deref(&self) -> &P {
        &self.inner
    }
}

pub fn to_plc(p: &Path) -> OsString {
    let mut s = p.components()
        .last()
        .map(|c| c.as_os_str() )
        .unwrap_or(OsStr::new(""))
        .to_owned();
    s.shrink_to_fit();
    s
}
