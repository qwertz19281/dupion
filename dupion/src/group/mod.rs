use super::*;
use util::{Size, Hash};
use vfs::{entry::VfsEntryType, VfsId};

#[derive(Clone)]
pub struct SizeGroup {
    pub entries: Vec<(VfsEntryType,VfsId)>,
    pub size: Size,
}

#[derive(Clone)]
pub struct HashGroup {
    pub entries: Vec<(VfsEntryType,VfsId)>,
    pub size: Size,
    pub hash: Hash,
}