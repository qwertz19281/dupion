use crate::archive::FileType;

use libc::{c_uint, mode_t, timespec};
use std::ffi::{CStr, CString};
use std::path::PathBuf;
use std::str;

use libarchive3_sys::ffi;

pub trait Entry {
    unsafe fn entry(&self) -> *mut ffi::Struct_archive_entry;

    fn atime(&self) -> Option<timespec> {
        let entry = unsafe { self.entry() };
        if 0 == unsafe { ffi::archive_entry_atime_is_set(entry) } {
            return None;
        }
        Some(timespec {
            tv_sec: unsafe { ffi::archive_entry_atime(entry) },
            tv_nsec: unsafe { ffi::archive_entry_atime_nsec(entry) },
        })
    }

    fn birthtime(&self) -> Option<timespec> {
        let entry = unsafe { self.entry() };
        if 0 == unsafe { ffi::archive_entry_birthtime_is_set(entry) } {
            return None;
        }
        Some(timespec {
            tv_sec: unsafe { ffi::archive_entry_birthtime(entry) },
            tv_nsec: unsafe { ffi::archive_entry_birthtime_nsec(entry) },
        })
    }

    fn ctime(&self) -> Option<timespec> {
        let entry = unsafe { self.entry() };
        if 0 == unsafe { ffi::archive_entry_ctime_is_set(entry) } {
            return None;
        }
        Some(timespec {
            tv_sec: unsafe { ffi::archive_entry_ctime(entry) },
            tv_nsec: unsafe { ffi::archive_entry_ctime_nsec(entry) },
        })
    }

    fn filetype(&self) -> FileType {
        unsafe {
            match ffi::archive_entry_filetype(self.entry()) as u32 {
                ffi::AE_IFBLK => FileType::BlockDevice,
                ffi::AE_IFCHR => FileType::CharacterDevice,
                ffi::AE_IFLNK => FileType::SymbolicLink,
                ffi::AE_IFDIR => FileType::Directory,
                ffi::AE_IFIFO => FileType::NamedPipe,
                ffi::AE_IFMT => FileType::Mount,
                ffi::AE_IFREG => FileType::RegularFile,
                ffi::AE_IFSOCK => FileType::Socket,
                0 => FileType::Unknown,
                code => unreachable!("undefined filetype: {}", code),
            }
        }
    }

    fn hardlink_raw(&self) -> Option<&[u8]> {
        let c_str: &CStr = unsafe {
            let ptr = ffi::archive_entry_hardlink_utf8(self.entry());
            if ptr.is_null() {
                return None;
            }
            CStr::from_ptr(ptr)
        };
        let buf: &[u8] = c_str.to_bytes();
        Some(buf)
    }

    fn hardlink(&self) -> Option<&str> {
        self.hardlink_raw().map(|buf| str::from_utf8(buf).unwrap())
    }

    fn mode(&self) -> mode_t {
        unsafe { ffi::archive_entry_mode(self.entry()) }
    }

    fn mtime(&self) -> Option<timespec> {
        let entry = unsafe { self.entry() };
        if 0 == unsafe { ffi::archive_entry_mtime_is_set(entry) } {
            return None;
        }
        Some(timespec {
            tv_sec: unsafe { ffi::archive_entry_mtime(entry) },
            tv_nsec: unsafe { ffi::archive_entry_mtime_nsec(entry) },
        })
    }

    fn nlink(&self) -> c_uint {
        unsafe { ffi::archive_entry_nlink(self.entry()) }
    }

    fn pathname_raw(&self) -> Option<&[u8]> {
        let c_str: &CStr = unsafe {
            let ptr = ffi::archive_entry_pathname_utf8(self.entry());
            if ptr.is_null() {
                return None;
            }
            CStr::from_ptr(ptr)
        };
        let buf: &[u8] = c_str.to_bytes();
        Some(buf)
    }

    fn pathname(&self) -> Option<&str> {
        self.pathname_raw().map(|buf| str::from_utf8(buf).unwrap())
    }

    fn size(&self) -> i64 {
        unsafe { ffi::archive_entry_size(self.entry()) }
    }

    fn symlink_raw(&self) -> Option<&[u8]> {
        let c_str: &CStr = unsafe {
            let ptr = ffi::archive_entry_symlink_utf8(self.entry());
            if ptr.is_null() {
                return None;
            }
            CStr::from_ptr(ptr)
        };
        let buf: &[u8] = c_str.to_bytes();
        Some(buf)
    }

    fn symlink(&self) -> Option<&str> {
        self.symlink_raw().map(|buf| str::from_utf8(buf).unwrap())
    }

    fn uname_raw(&self) -> Option<&[u8]> {
        let c_str: &CStr = unsafe {
            let ptr = ffi::archive_entry_uname_utf8(self.entry());
            if ptr.is_null() {
                return None;
            }
            CStr::from_ptr(ptr)
        };
        let buf: &[u8] = c_str.to_bytes();
        Some(buf)
    }

    fn uname(&self) -> Option<&str> {
        self.uname_raw().map(|buf| str::from_utf8(buf).unwrap())
    }

    fn set_atime(&mut self, t: Option<timespec>) {
        match t {
            Some(t) => unsafe { ffi::archive_entry_set_atime(self.entry(), t.tv_sec, t.tv_nsec) },
            None => unsafe { ffi::archive_entry_unset_atime(self.entry()) },
        }
    }

    fn set_birthtime(&mut self, t: Option<timespec>) {
        match t {
            Some(t) => unsafe { ffi::archive_entry_set_birthtime(self.entry(), t.tv_sec, t.tv_nsec) },
            None => unsafe { ffi::archive_entry_unset_birthtime(self.entry()) },
        }
    }

    fn set_ctime(&mut self, t: Option<timespec>) {
        match t {
            Some(t) => unsafe { ffi::archive_entry_set_ctime(self.entry(), t.tv_sec, t.tv_nsec) },
            None => unsafe { ffi::archive_entry_unset_ctime(self.entry()) },
        }
    }

    fn set_filetype(&mut self, file_type: FileType) {
        unsafe {
            let file_type = match file_type {
                FileType::BlockDevice => ffi::AE_IFBLK,
                FileType::CharacterDevice => ffi::AE_IFCHR,
                FileType::SymbolicLink => ffi::AE_IFLNK,
                FileType::Directory => ffi::AE_IFDIR,
                FileType::NamedPipe => ffi::AE_IFIFO,
                FileType::Mount => ffi::AE_IFMT,
                FileType::RegularFile => ffi::AE_IFREG,
                FileType::Socket => ffi::AE_IFSOCK,
                FileType::Unknown => 0,
            };
            ffi::archive_entry_set_filetype(self.entry(), file_type);
        }
    }

    fn set_link(&mut self, path: &PathBuf) {
        unsafe {
            let c_str = CString::new(path.to_str().unwrap()).unwrap();
            ffi::archive_entry_set_link_utf8(self.entry(), c_str.as_ptr());
        }
    }

    fn set_mode(&mut self, m: mode_t) {
        unsafe { ffi::archive_entry_set_mode(self.entry(), m) };
    }

    fn set_mtime(&mut self, t: Option<timespec>) {
        match t {
            Some(t) => unsafe { ffi::archive_entry_set_mtime(self.entry(), t.tv_sec, t.tv_nsec) },
            None => unsafe { ffi::archive_entry_unset_mtime(self.entry()) },
        }
    }

    fn set_nlink(&mut self, n: c_uint) {
        unsafe { ffi::archive_entry_set_nlink(self.entry(), n) }
    }

    fn set_pathname(&mut self, path: &PathBuf) {
        unsafe {
            let c_str = CString::new(path.to_str().unwrap()).unwrap();
            ffi::archive_entry_set_pathname_utf8(self.entry(), c_str.as_ptr());
        }
    }
}

pub fn entry_debug_fmt<E: Entry>(struct_name: &str, e: &E, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
    f.debug_struct(struct_name)
        .field("type", &e.filetype())
        .field("pathname", &e.pathname_raw().map_or("".to_owned(),|s| String::from_utf8_lossy(s).into_owned() ) )
        .finish()
}
