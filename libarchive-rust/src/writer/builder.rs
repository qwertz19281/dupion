use std::default::Default;
use std::path::Path;
use std::{marker::PhantomData, ffi::CString};

use libarchive3_sys::ffi;

use crate::archive::{ArchiveHandle, Handle, WriteFilter, WriteFormat};
use crate::error::{ArchiveResult, ArchiveError};
use crate::writer::writer::Writer;

pub struct Builder<'r> {
    handle: ArchiveHandle<'r>,
    _p: PhantomData<&'r mut ()>,
}

impl<'r> Builder<'r> {
    pub fn new() -> Self {
        Builder::default()
    }

    pub fn add_filter(&mut self, filter: WriteFilter) -> ArchiveResult<()> {
        let result = match filter {
            WriteFilter::B64Encode => unsafe {
                ffi::archive_write_add_filter_b64encode(self.handle())
            },
            WriteFilter::Bzip2 => unsafe { ffi::archive_write_add_filter_bzip2(self.handle()) },
            WriteFilter::Compress => unsafe { ffi::archive_write_add_filter_compress(self.handle()) },
            WriteFilter::Grzip => unsafe { ffi::archive_write_add_filter_grzip(self.handle()) },
            WriteFilter::Gzip => unsafe { ffi::archive_write_add_filter_gzip(self.handle()) },
            WriteFilter::Lrzip => unsafe { ffi::archive_write_add_filter_lrzip(self.handle()) },
            WriteFilter::Lzip => unsafe { ffi::archive_write_add_filter_lzip(self.handle()) },
            WriteFilter::Lzma => unsafe { ffi::archive_write_add_filter_lzma(self.handle()) },
            WriteFilter::Lzop => unsafe { ffi::archive_write_add_filter_lzop(self.handle()) },
            WriteFilter::None => unsafe { ffi::archive_write_add_filter_none(self.handle()) },
            WriteFilter::Program(prog) => {
                let c_prog = CString::new(prog).unwrap();
                unsafe { ffi::archive_write_add_filter_program(self.handle(), c_prog.as_ptr()) }
            }
            WriteFilter::UuEncode => unsafe { ffi::archive_write_add_filter_uuencode(self.handle()) },
            WriteFilter::Xz => unsafe { ffi::archive_write_add_filter_xz(self.handle()) },
        };
        match result {
            ffi::ARCHIVE_OK => Ok(()),
            _ => ArchiveResult::from(self as &dyn Handle),
        }
    }

    pub fn set_format(&self, format: WriteFormat) -> ArchiveResult<()> {
        let result = match format {
            WriteFormat::SevenZip => unsafe { ffi::archive_write_set_format_7zip(self.handle()) },
            WriteFormat::ArBsd => unsafe { ffi::archive_write_set_format_ar_bsd(self.handle()) },
            WriteFormat::ArSvr4 => unsafe { ffi::archive_write_set_format_ar_svr4(self.handle()) },
            WriteFormat::Cpio => unsafe { ffi::archive_write_set_format_cpio(self.handle()) },
            WriteFormat::CpioNewc => unsafe {
                ffi::archive_write_set_format_cpio_newc(self.handle())
            },
            WriteFormat::Gnutar => unsafe { ffi::archive_write_set_format_gnutar(self.handle()) },
            WriteFormat::Iso9660 => unsafe { ffi::archive_write_set_format_iso9660(self.handle()) },
            WriteFormat::Mtree => unsafe { ffi::archive_write_set_format_mtree(self.handle()) },
            WriteFormat::MtreeClassic => unsafe {
                ffi::archive_write_set_format_mtree_classic(self.handle())
            },
            WriteFormat::Pax => unsafe { ffi::archive_write_set_format_pax(self.handle()) },
            WriteFormat::PaxRestricted => unsafe {
                ffi::archive_write_set_format_pax_restricted(self.handle())
            },
            WriteFormat::Shar => unsafe { ffi::archive_write_set_format_shar(self.handle()) },
            WriteFormat::SharDump => unsafe {
                ffi::archive_write_set_format_shar_dump(self.handle())
            },
            WriteFormat::Ustar => unsafe { ffi::archive_write_set_format_ustar(self.handle()) },
            WriteFormat::V7tar => unsafe { ffi::archive_write_set_format_v7tar(self.handle()) },
            WriteFormat::Xar => unsafe { ffi::archive_write_set_format_xar(self.handle()) },
            WriteFormat::Zip => unsafe { ffi::archive_write_set_format_zip(self.handle()) },
        };
        match result {
            ffi::ARCHIVE_OK => Ok(()),
            _ => ArchiveResult::from(self as &dyn Handle),
        }
    }

    pub fn open_file<T: AsRef<Path>>(self, file: T) -> ArchiveResult<Writer<'r>> {
        let c_file = CString::new(file.as_ref().to_string_lossy().as_bytes()).unwrap();
        let res = unsafe { ffi::archive_write_open_filename(self.handle(), c_file.as_ptr()) };
        match res {
            ffi::ARCHIVE_OK => {
                Ok(Writer::new(self.handle))
            }
            _ => Err(ArchiveError::from(&self as &dyn Handle)),
        }
    }
}

impl<'r> Default for Builder<'r> {
    fn default() -> Self {
        unsafe {
            let handle = ArchiveHandle::from_raw(ffi::archive_write_new());
            Builder { handle: handle.expect("Allocation error"), _p: PhantomData }
        }
    }
}

impl<'r> Handle<'r> for Builder<'r> {
    unsafe fn handle(&self) -> &mut ffi::Struct_archive {
        self.handle.handle()
    }
}
