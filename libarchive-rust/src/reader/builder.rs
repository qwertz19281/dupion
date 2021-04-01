use std::default::Default;
use std::ffi::CString;
use std::io::{Read, Seek};
use std::mem;
use std::{marker::PhantomData, path::Path};

use libarchive3_sys::ffi;

use crate::archive::{ArchiveHandle, ReadCompression, ReadFilter, ReadFormat, Handle};
use crate::error::ArchiveResult;
use super::{FileReader, StreamReader};

pub struct Builder<'r> {
    handle: ArchiveHandle<'r>,
    _p: PhantomData<&'r mut ()>,
}

impl<'r> Builder<'r> {
    pub fn new() -> Self {
        Builder::default()
    }

    pub fn support_compression(&mut self, compression: ReadCompression) -> ArchiveResult<()> {
        let result = match compression {
            ReadCompression::All => unsafe {
                ffi::archive_read_support_compression_all(self.handle())
            },
            ReadCompression::Bzip2 => unsafe {
                ffi::archive_read_support_compression_bzip2(self.handle())
            },
            ReadCompression::Compress => unsafe {
                ffi::archive_read_support_compression_compress(self.handle())
            },
            ReadCompression::Gzip => unsafe {
                ffi::archive_read_support_compression_gzip(self.handle())
            },
            ReadCompression::Lzip => unsafe {
                ffi::archive_read_support_compression_lzip(self.handle())
            },
            ReadCompression::Lzma => unsafe {
                ffi::archive_read_support_compression_lzma(self.handle())
            },
            ReadCompression::None => unsafe {
                ffi::archive_read_support_compression_none(self.handle())
            },
            ReadCompression::Program(prog) => {
                let c_prog = CString::new(prog).unwrap();
                unsafe {
                    ffi::archive_read_support_compression_program(self.handle(), c_prog.as_ptr())
                }
            }
            ReadCompression::Rpm => unsafe {
                ffi::archive_read_support_compression_rpm(self.handle())
            },
            ReadCompression::Uu => unsafe { ffi::archive_read_support_compression_uu(self.handle()) },
            ReadCompression::Xz => unsafe { ffi::archive_read_support_compression_xz(self.handle()) },
        };
        match result {
            ffi::ARCHIVE_OK => Ok(()),
            _ => ArchiveResult::from(self as &dyn Handle),
        }
    }

    pub fn support_filter(&mut self, filter: ReadFilter) -> ArchiveResult<()> {
        let result = match filter {
            ReadFilter::All => unsafe { ffi::archive_read_support_filter_all(self.handle()) },
            ReadFilter::Bzip2 => unsafe { ffi::archive_read_support_filter_bzip2(self.handle()) },
            ReadFilter::Compress => unsafe {
                ffi::archive_read_support_filter_compress(self.handle())
            },
            ReadFilter::Grzip => unsafe { ffi::archive_read_support_filter_grzip(self.handle()) },
            ReadFilter::Gzip => unsafe { ffi::archive_read_support_filter_gzip(self.handle()) },
            ReadFilter::Lrzip => unsafe { ffi::archive_read_support_filter_lrzip(self.handle()) },
            ReadFilter::Lzip => unsafe { ffi::archive_read_support_filter_lzip(self.handle()) },
            ReadFilter::Lzma => unsafe { ffi::archive_read_support_filter_lzma(self.handle()) },
            ReadFilter::Lzop => unsafe { ffi::archive_read_support_filter_lzop(self.handle()) },
            ReadFilter::None => unsafe { ffi::archive_read_support_filter_none(self.handle()) },
            ReadFilter::Program(prog) => {
                let c_prog = CString::new(prog).unwrap();
                unsafe { ffi::archive_read_support_filter_program(self.handle(), c_prog.as_ptr()) }
            }
            ReadFilter::ProgramSignature(prog, cb, size) => {
                let c_prog = CString::new(prog).unwrap();
                unsafe {
                    ffi::archive_read_support_filter_program_signature(self.handle(),
                                                                       c_prog.as_ptr(),
                                                                       mem::transmute(cb),
                                                                       size)
                }
            }
            ReadFilter::Rpm => unsafe { ffi::archive_read_support_filter_rpm(self.handle()) },
            ReadFilter::Uu => unsafe { ffi::archive_read_support_filter_uu(self.handle()) },
            ReadFilter::Xz => unsafe { ffi::archive_read_support_filter_xz(self.handle()) },
        };
        match result {
            ffi::ARCHIVE_OK => Ok(()),
            _ => ArchiveResult::from(self as &dyn Handle),
        }
    }

    pub fn support_format(&self, format: ReadFormat) -> ArchiveResult<()> {
        let result = match format {
            ReadFormat::SevenZip => unsafe { ffi::archive_read_support_format_7zip(self.handle()) },
            ReadFormat::All => unsafe { ffi::archive_read_support_format_all(self.handle()) },
            ReadFormat::Ar => unsafe { ffi::archive_read_support_format_ar(self.handle()) },
            ReadFormat::Cab => unsafe { ffi::archive_read_support_format_cab(self.handle()) },
            ReadFormat::Cpio => unsafe { ffi::archive_read_support_format_cpio(self.handle()) },
            ReadFormat::Empty => unsafe { ffi::archive_read_support_format_empty(self.handle()) },
            ReadFormat::Gnutar => unsafe { ffi::archive_read_support_format_gnutar(self.handle()) },
            ReadFormat::Iso9660 => unsafe {
                ffi::archive_read_support_format_iso9660(self.handle())
            },
            ReadFormat::Lha => unsafe { ffi::archive_read_support_format_lha(self.handle()) },
            ReadFormat::Mtree => unsafe { ffi::archive_read_support_format_mtree(self.handle()) },
            ReadFormat::Rar => unsafe { ffi::archive_read_support_format_rar(self.handle()) },
            ReadFormat::Raw => unsafe { ffi::archive_read_support_format_raw(self.handle()) },
            ReadFormat::Tar => unsafe { ffi::archive_read_support_format_tar(self.handle()) },
            ReadFormat::Xar => unsafe { ffi::archive_read_support_format_xar(self.handle()) },
            ReadFormat::Zip => unsafe { ffi::archive_read_support_format_zip(self.handle()) },
        };
        match result {
            ffi::ARCHIVE_OK => Ok(()),
            _ => ArchiveResult::from(self as &dyn Handle),
        }
    }

    pub fn open_file<T: AsRef<Path>>(self, file: T) -> ArchiveResult<FileReader<'r>> {
        FileReader::open(self, file)
    }

    #[cfg(unix)]
    pub fn open_fd(self, fd: &::std::os::unix::io::RawFd) -> ArchiveResult<FileReader<'r>> {
        FileReader::open_fd(self, fd)
    }

    pub fn open_seekable_stream<T: 'r + Read + Seek>(self, src: T) -> ArchiveResult<StreamReader<'r,T>> {
        StreamReader::open_seekable(self, src)
    }

    pub fn open_stream<T: 'r + Read>(self, src: T) -> ArchiveResult<StreamReader<'r,T>> {
        StreamReader::open(self, src)
    }
}

impl<'r> From<Builder<'r>> for ArchiveHandle<'r> {
    fn from(b: Builder<'r>) -> ArchiveHandle<'r> {
        b.handle
    }
}

impl<'r> Handle<'r> for Builder<'r> {
    unsafe fn handle(&self) -> &mut ffi::Struct_archive {
        self.handle.handle()
    }
}

impl<'r> Default for Builder<'r> {
    fn default() -> Self {
        unsafe {
            let handle = ArchiveHandle::from_raw(ffi::archive_read_new());
            Builder { handle: handle.expect("Allocation error"), _p: PhantomData }
        }
    }
}
