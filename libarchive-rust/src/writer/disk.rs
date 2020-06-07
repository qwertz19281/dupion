use std::default::Default;
use std::path::Path;
use std::{marker::PhantomData, ptr};

use libarchive3_sys::ffi;

use archive::{ArchiveHandle, ExtractOptions, Handle};
use reader::Reader;
use entry::{Entry, BorrowedEntry};
use error::{ArchiveResult, ArchiveError};

pub struct Disk<'r> {
    handle: ArchiveHandle<'r>,
    _p: PhantomData<&'r mut ()>,
}

impl<'r> Disk<'r> {
    pub fn new() -> Self {
        Disk::default()
    }

    // Retrieve the currently-set value for last block size. A value of -1 here indicates that the
    // library should use default values.
    pub fn bytes_in_last_block(&self) -> i32 {
        unsafe { ffi::archive_write_get_bytes_in_last_block(self.handle()) }
    }

    // Retrieve the block size to be used for writing. A value of -1 here indicates that the
    // library should use default values. A value of zero indicates that internal blocking is
    // suppressed.
    pub fn bytes_per_block(&self) -> i32 {
        unsafe { ffi::archive_write_get_bytes_per_block(self.handle()) }
    }

    pub fn set_bytes_per_block(&mut self, count: i32) -> ArchiveResult<()> {
        unsafe {
            match ffi::archive_write_set_bytes_per_block(self.handle(), count) {
                ffi::ARCHIVE_OK => Ok(()),
                _ => ArchiveResult::from(self as &dyn Handle),
            }
        }
    }

    pub fn set_bytes_in_last_block(&mut self, count: i32) -> ArchiveResult<()> {
        unsafe {
            match ffi::archive_write_set_bytes_in_last_block(self.handle(), count) {
                ffi::ARCHIVE_OK => Ok(()),
                _ => ArchiveResult::from(self as &dyn Handle),
            }
        }
    }

    // Set options for extraction built from `ExtractOptions`
    pub fn set_options(&self, eopt: &ExtractOptions) -> ArchiveResult<()> {
        unsafe {
            match ffi::archive_write_disk_set_options(self.handle(), eopt.flags) {
                ffi::ARCHIVE_OK => Ok(()),
                _ => ArchiveResult::from(self as &dyn Handle),
            }
        }
    }

    // This convenience function installs a standard set of user and group lookup functions. These
    // functions use getpwnam(3) and getgrnam(3) to convert names to ids, defaulting to the ids if
    // the names cannot be looked up. These functions also implement a simple memory cache to
    // reduce the number of calls to getpwnam(3) and getgrnam(3).
    pub fn set_standard_lookup(&self) -> ArchiveResult<()> {
        unsafe {
            match ffi::archive_write_disk_set_standard_lookup(self.handle()) {
                ffi::ARCHIVE_OK => Ok(()),
                _ => ArchiveResult::from(self as &dyn Handle),
            }
        }
    }

    // * Failures - HeaderPosition
    pub fn write<T: Reader<'r>>(&self, reader: &mut T, prefix: Option<&str>) -> ArchiveResult<usize> {
        if reader.header_position() != 0 {
            return Err(ArchiveError::HeaderPosition);
        }
        let mut bytes: usize = 0;
        let mut write_pending: bool = false;
        loop {
            {
                if let Some(entry) = reader.next_header() {
                    if let Some(pfx) = prefix {
                        let path = Path::new(pfx).join(entry.pathname().expect("TODO"));
                        entry.set_pathname(&path);
                        if entry.hardlink().is_some() {
                            let path = Path::new(pfx).join(entry.hardlink().unwrap());
                            entry.set_link(&path);
                        }
                    }
                    match self.write_header(entry) {
                        Ok(()) => (),
                        Err(e) => return Err(e),
                    }
                    if entry.size() > 0 {
                        write_pending = true
                    }
                } else {
                    break;
                }
            }
            if write_pending {
                bytes += try!(self.write_data(reader));
                write_pending = false;
            }
        }
        unsafe {
            match ffi::archive_write_finish_entry(self.handle()) {
                ffi::ARCHIVE_OK => Ok(bytes),
                _ => Err(ArchiveError::from(self as &dyn Handle)),
            }
        }
    }

    pub fn close(&self) -> ArchiveResult<()> {
        unsafe {
            match ffi::archive_write_close(self.handle()) {
                ffi::ARCHIVE_OK => Ok(()),
                _ => ArchiveResult::from(self as &dyn Handle),
            }
        }
    }

    fn write_data<T: Reader<'r>>(&self, reader: &T) -> ArchiveResult<usize> {
        let mut buff = ptr::null();
        let mut size = 0;
        let mut offset = 0;

        unsafe {
            loop {
                match ffi::archive_read_data_block(reader.handle(),
                                                   &mut buff,
                                                   &mut size,
                                                   &mut offset) {
                    ffi::ARCHIVE_EOF => return Ok(size),
                    ffi::ARCHIVE_OK => {
                        if ffi::archive_write_data_block(self.handle(), buff, size, offset) !=
                           ffi::ARCHIVE_OK as isize {
                            return Err(ArchiveError::from(self as &dyn Handle));
                        }
                    }
                    _ => return Err(ArchiveError::from(reader as &dyn Handle)),
                }
            }
        }
    }

    fn write_header(&self, entry: &BorrowedEntry) -> ArchiveResult<()> {
        unsafe {
            match ffi::archive_write_header(self.handle(), entry.entry()) {
                ffi::ARCHIVE_OK => Ok(()),
                _ => ArchiveResult::from(self as &dyn Handle),
            }
        }
    }
}

impl<'r> Handle<'r> for Disk<'r> {
    unsafe fn handle(&self) -> &mut ffi::Struct_archive {
        self.handle.handle()
    }
}

impl<'r> Default for Disk<'r> {
    fn default() -> Self {
        unsafe {
            let handle = ArchiveHandle::from_raw(ffi::archive_write_disk_new());
            Disk { handle: handle.expect("Allocation error"), _p: PhantomData }
        }
    }
}
