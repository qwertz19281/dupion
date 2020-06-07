use std::default::Default;
use std::ffi::CString;
use std::{marker::PhantomData, path::Path};

use libarchive3_sys::ffi;

use archive::{ArchiveHandle, Handle};
use entry::BorrowedEntry;
use error::{ArchiveResult, ArchiveError};
use super::{Builder, Reader};

const BLOCK_SIZE: usize = 10240;

pub struct FileReader<'r> {
    handle: ArchiveHandle<'r>,
    entry: BorrowedEntry,
    _p: PhantomData<&'r mut ()>,
}

impl<'r> FileReader<'r> {
    pub fn open<T: AsRef<Path>>(builder: Builder<'r>, file: T) -> ArchiveResult<Self> {
        let c_file = CString::new(file.as_ref().to_string_lossy().as_bytes()).unwrap();
        unsafe {
            match ffi::archive_read_open_filename(builder.handle(), c_file.as_ptr(), BLOCK_SIZE) {
                ffi::ARCHIVE_OK => {
                    Ok(Self::new(builder.into()))
                }
                _ => Err(ArchiveError::from(&builder as &dyn Handle)),
            }
        }
    }

    /// Opens archive backed by given file descriptor.
    /// Note that the file descriptor is not owned, i.e. it won't be closed
    /// on destruction of FileReader.
    /// It's your responsibility to close the descriptor after it's no longer used by FileReader.
    /// This is hinted at by taking RawFd by reference.
    #[cfg(unix)]
    pub fn open_fd(builder: Builder<'r>, fd: &::std::os::unix::io::RawFd) -> ArchiveResult<Self> {
        unsafe {
            match ffi::archive_read_open_fd(builder.handle(), *fd, BLOCK_SIZE) {
                ffi::ARCHIVE_OK => {
                    Ok(Self::new(builder.into()))
                }
                _ => Err(ArchiveError::from(&builder as &dyn Handle)),
            }
        }
    }

    fn new(handle: ArchiveHandle<'r>) -> Self {
        FileReader {
            handle: handle,
            entry: BorrowedEntry::default(),
            _p: PhantomData,
        }
    }
}

impl<'r> Handle<'r> for FileReader<'r> {
    unsafe fn handle(&self) -> &mut ffi::Struct_archive {
        self.handle.handle()
    }
}

impl<'r> Reader<'r> for FileReader<'r> {
    fn entry(&mut self) -> &mut BorrowedEntry {
        &mut self.entry
    }
}