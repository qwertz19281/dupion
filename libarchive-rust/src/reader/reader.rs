use std::ptr;
use std::slice;

use libc::{off_t, size_t};
use libarchive3_sys::ffi;

use archive::Handle;
use entry::{Entry, BorrowedEntry, OwnedEntry};
use error::{ArchiveResult, ArchiveError};

#[deprecated(note="Use BorrowedEntry directly instead.")]
pub use entry::BorrowedEntry as ReaderEntry;

pub trait Reader<'r>: Handle<'r> {
    fn entry(&mut self) -> &mut BorrowedEntry;

    fn header_position(&self) -> i64 {
        unsafe { ffi::archive_read_header_position(self.handle()) }
    }

    fn next_header(&mut self) -> Option<&mut BorrowedEntry> {
        let res = unsafe { ffi::archive_read_next_header(self.handle(), &mut self.entry().handle) };
        if res == ffi::ARCHIVE_OK {
            Some(self.entry())
        } else {
            None
        }
    }

    fn next_header2(&mut self, entry: &mut OwnedEntry) -> ArchiveResult<bool> {
        let res = unsafe { ffi::archive_read_next_header2(self.handle(), entry.entry()) };
        match res {
            ffi::ARCHIVE_OK => Ok(true),
            ffi::ARCHIVE_EOF => Ok(false),
            _ => Err(ArchiveError::Sys(self.err_code(), self.err_msg())),
        }
    }

    fn read(&mut self, buffer: &mut [u8]) -> ArchiveResult<size_t> {
        let ret_val = unsafe { ffi::archive_read_data(self.handle(), buffer.as_mut_ptr() as *mut _, buffer.len()) };
        if ret_val >= 0 {
            return Ok(ret_val as size_t);
        }

        Err(ArchiveError::Sys(self.err_code(), self.err_msg()))
    }

    fn read_all(&mut self) -> ArchiveResult<Vec<u8>> {
        const INCREMENT : usize = 65536;
        let mut buf = Vec::with_capacity(INCREMENT);
        loop {
            let len = buf.len();
            let mut cap = buf.capacity();
            if len >= cap {
                buf.reserve(len + INCREMENT);
                cap = buf.capacity();
            }

            let res = self.read(unsafe { buf.get_unchecked_mut(len..cap) })?;
            if 0 == res {
                break; //EOF
            }
            unsafe { buf.set_len(len + res) };
        };
        Ok(buf)
    }

    fn read_exact(&mut self, buf: &mut [u8]) -> ArchiveResult<usize> {
        let cnt = buf.len();
        let mut read_total: usize = 0;
        while read_total < cnt {
            let read_bytes = self.read(unsafe { buf.get_unchecked_mut(read_total..cnt) })?;
            if 0 == read_bytes {
                break; //EOF
            }
            read_total += read_bytes as usize;
        };
        Ok(read_total)
    }

    fn skip_exact(&mut self, cnt: usize) -> ArchiveResult<usize> {
        let mut tmp_buf = [0; 4096];
        let mut read_total: usize = 0;
        while read_total < cnt {
            let to_read = ::std::cmp::min(tmp_buf.len(), cnt);
            let read_bytes = self.read(unsafe { tmp_buf.get_unchecked_mut(0..to_read) })?;
            if 0 == read_bytes {
                break; //EOF
            }
            read_total += read_bytes as usize;
        }
        Ok(read_total)
    }

    fn read_block(&mut self) -> ArchiveResult<Option<(&[u8], off_t)>> {
        let mut buff = ptr::null();
        let mut size = 0;
        let mut offset = 0;

        unsafe {
            match ffi::archive_read_data_block(self.handle(), &mut buff, &mut size, &mut offset) {
                ffi::ARCHIVE_EOF => Ok(None),
                ffi::ARCHIVE_OK => Ok(Some((slice::from_raw_parts(buff as *const u8, size), offset))),
                _ => Err(ArchiveError::Sys(self.err_code(), self.err_msg())),
            }
        }
    }

    fn read_skip(&mut self) -> ArchiveResult<()> {
        let res = unsafe { ffi::archive_read_data_skip(self.handle()) };
        if res == ffi::ARCHIVE_OK {
            Ok(())
        } else {
            Err(ArchiveError::Sys(self.err_code(), self.err_msg()))
        }
    }
}

