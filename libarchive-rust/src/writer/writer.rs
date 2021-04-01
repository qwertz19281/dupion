use libarchive3_sys::ffi;

use crate::archive::{ArchiveHandle, Handle};
use std::marker::PhantomData;

pub struct Writer<'r> {
    handle: ArchiveHandle<'r>,
    _p: PhantomData<&'r mut ()>,
}

impl<'r> Writer<'r> {
    pub(crate) fn new(handle: ArchiveHandle<'r>) -> Self {
        Writer { handle: handle, _p: PhantomData }
    }
}

impl<'r> Handle<'r> for Writer<'r> {
    unsafe fn handle(&self) -> &mut ffi::Struct_archive {
        self.handle.handle()
    }
}