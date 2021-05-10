use super::*;
use std::{ffi::CString, io::ErrorKind, path::Path};

pub struct FileDescriptor {
    value: libc::c_int,
}

impl FileDescriptor {
    pub fn open<AsPath: AsRef<Path>>(
        path: AsPath,
        flags: libc::c_int,
    ) -> Result<FileDescriptor, String> {
        let path = path.as_ref();

        // TODO should be able to do this cleanly on linux...

        let path_string = path
            .to_str()
            .ok_or("Invalid characters")?
            .to_owned();

        let path_c = CString::new(path_string.into_bytes())
            .map_err(|_| "Invalid characters")?;

        let fd = cvt_r(|| unsafe { libc::open(path_c.as_ptr(), flags) })
            .map_err(|e| format!("OS Error: {}",e) )?;

        if fd >= 0 {
            Ok(FileDescriptor { value: fd })
        } else {
            Err(format!("error opening {:?}", path))
        }
    }

    pub fn get_value(&self) -> libc::c_int {
        self.value
    }
}

impl Drop for FileDescriptor {
    fn drop(&mut self) {
        unsafe {
            libc::close(self.value);
        }
    }
}

impl<'a> From<&'a FileDescriptor> for libc::c_int {
    fn from(file_descriptor: &'a FileDescriptor) -> libc::c_int {
        file_descriptor.value
    }
}

pub trait IsMinusOne {
    fn is_minus_one(&self) -> bool;
}

macro_rules! impl_is_minus_one {
    ($($t:ident)*) => ($(impl IsMinusOne for $t {
        fn is_minus_one(&self) -> bool {
            *self == -1
        }
    })*)
}

impl_is_minus_one! { i8 i16 i32 i64 isize }

pub fn cvt<T: IsMinusOne>(t: T) -> std::io::Result<T> {
    if t.is_minus_one() {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(t)
    }
}

pub fn cvt_r<T, F>(mut f: F) -> std::io::Result<T>
where
    T: IsMinusOne,
    F: FnMut() -> T,
{
    loop {
        match cvt(f()) {
            Err(ref e) if e.kind() == ErrorKind::Interrupted => {}
            other => return other,
        }
    }
}
