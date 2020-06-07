use std::default::Default;
use std::error::Error;
use std::ffi::CString;
use std::{marker::PhantomData, io::{self, Read, Seek, SeekFrom}};

use libc::{c_void, ssize_t, c_int, int64_t, SEEK_SET, SEEK_CUR, SEEK_END};
use libarchive3_sys::ffi;

use archive::{ArchiveHandle, Handle};
use entry::BorrowedEntry;
use error::{ArchiveResult, ArchiveError};
use super::{Builder, Reader};

pub struct StreamReader<'r,T> where T: 'r {
    handle: ArchiveHandle<'r>,
    entry: BorrowedEntry,
    _pipe: Box<Pipe<'r,T>>,
}

struct Pipe<'r,T> where T: 'r {
    reader: T,
    buffer: Vec<u8>,
    _p: PhantomData<&'r mut T>,
}

impl<'r,T> Pipe<'r,T> where T: 'r {
    fn new(src: T) -> Self {
        Pipe {
            reader: src,
            buffer: vec![0; 8192],
            _p: PhantomData,
        }
    }

    fn read_bytes(&mut self) -> io::Result<usize> where T: Read {
        self.reader.read(&mut self.buffer[..])
    }

    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> where T: Seek {
        self.reader.seek(pos)
    }
}

impl<'r,T> StreamReader<'r,T> where T: 'r {
    pub fn open(builder: Builder<'r>, src: T) -> ArchiveResult<Self> where T: Read {
        unsafe {
            let mut pipe = Box::new(Pipe::new(src));
            let pipe_ptr: *mut c_void = &mut *pipe as *mut Pipe<T> as *mut c_void;
            match ffi::archive_read_open(builder.handle(),
                                         pipe_ptr,
                                         None,
                                         Some(stream_read_callback::<T>),
                                         None) {
                ffi::ARCHIVE_OK => {
                    let reader = StreamReader {
                        handle: builder.into(),
                        entry: BorrowedEntry::default(),
                        _pipe: pipe,
                    };
                    Ok(reader)
                }
                _ => {
                    Err(ArchiveError::from(&builder as &dyn Handle))
                }
            }
        }
    }

    pub fn open_seekable(builder: Builder<'r>, src: T) -> ArchiveResult<Self> where T: Read + Seek {
        unsafe {
            // Seek callback setter must be called before archive_read_open()
            match ffi::archive_read_set_seek_callback(builder.handle(), Some(stream_seek_callback::<T>)) {
                ffi::ARCHIVE_OK => {},
                _ => { return Err(ArchiveError::from(&builder as &dyn Handle)) },
            }
        };
        Self::open(builder, src)
    }

    pub fn into_inner(self) -> T {
        self._pipe.reader
    }
}

impl<'r,T> Handle<'r> for StreamReader<'r,T> where T: 'r {
    unsafe fn handle(&self) -> &mut ffi::Struct_archive {
        self.handle.handle()
    }
}

impl<'r,T> Reader<'r> for StreamReader<'r,T> where T: 'r {
    fn entry(&mut self) -> &mut BorrowedEntry {
        &mut self.entry
    }
}

unsafe extern "C" fn stream_read_callback<T: Read>(handle: *mut ffi::Struct_archive,
                                                   data: *mut c_void,
                                                   buff: *mut *const c_void)
                                                   -> ssize_t {
    let pipe: &mut Pipe<T> = &mut *(data as *mut Pipe<T>);
    *buff = pipe.buffer.as_mut_ptr() as *mut c_void;
    match pipe.read_bytes() {
        Ok(size) => size as ssize_t,
        Err(e) => {
            let desc = CString::new(e.description()).unwrap();
            ffi::archive_set_error(handle, e.raw_os_error().unwrap_or(0), desc.as_ptr());
            -1 as ssize_t
        }
    }
}

unsafe extern "C" fn stream_seek_callback<T: Seek>(handle: *mut ffi::Struct_archive,
                                                   data: *mut c_void,
                                                   offset: int64_t, whence: c_int)
                                                   -> int64_t {
    let pipe: &mut Pipe<T> = &mut *(data as *mut Pipe<T>);

    let pos = match whence {
        SEEK_SET => SeekFrom::Start(offset as u64),
        SEEK_CUR => SeekFrom::Current(offset),
        SEEK_END => SeekFrom::End(offset),
        // Panicking in C callback is UB, but meh. Not going to happen.
        _ => unreachable!("Invalid seek constant {}", whence),
    };

    match pipe.seek(pos) {
        Ok(new_pos) => new_pos as int64_t,
        Err(e) => {
            let desc = CString::new(e.description()).unwrap();
            ffi::archive_set_error(handle, e.raw_os_error().unwrap_or(0), desc.as_ptr());
            ffi::ARCHIVE_FATAL as int64_t
        }
    }
}
