use std::ffi::CStr;
use std::str;

use libarchive3_sys::ffi;
use crate::error::ErrCode;

pub trait Handle<'r> {
    unsafe fn handle(&self) -> &mut ffi::Struct_archive;

    fn err_code(&self) -> ErrCode {
        let code = unsafe { ffi::archive_errno(self.handle()) };
        ErrCode(code)
    }

    fn err_msg(&self) -> Option<String> {
        unsafe {
            let c_str = ffi::archive_error_string(self.handle());
            c_str.as_ref().map(|c_str| {
                let c_str = CStr::from_ptr(c_str);
                let buf = c_str.to_bytes();
                String::from(str::from_utf8(buf).unwrap())
            })
        }
    }
}

