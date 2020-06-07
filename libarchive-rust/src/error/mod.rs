use std::error;
use std::fmt;
use archive;

pub type ArchiveResult<T> = Result<T, ArchiveError>;

#[derive(Clone, Copy, PartialEq, PartialOrd, Eq, Ord, Debug, Hash)]
pub struct ErrCode(pub i32);

impl fmt::Display for ErrCode {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "{}", self.0)
    }
}

#[derive(Clone, PartialEq, PartialOrd, Eq, Ord, Debug, Hash)]
pub enum ArchiveError {
    HeaderPosition,
    Sys(ErrCode, Option<String>),
}

impl error::Error for ArchiveError {
    fn description(&self) -> &str {
        match self {
            &ArchiveError::HeaderPosition => "Header position expected to be 0",
            &ArchiveError::Sys(_, _) => "libarchive system error",
        }
    }
}

impl fmt::Display for ArchiveError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            &ArchiveError::HeaderPosition => write!(fmt, "Header position expected to be 0"),
            &ArchiveError::Sys(ref code, ref msg) => {
                if let &Some(ref msg) = msg {
                    write!(fmt, "{} (libarchive err_code={})", msg, code)
                } else {
                    write!(fmt, "(no message) (libarchive err_code={})", code)
                }
            }
        }
    }
}

impl<'a,'r> From<&'a archive::Handle<'r>> for ArchiveError {
    fn from(handle: &'a archive::Handle) -> ArchiveError {
        ArchiveError::Sys(handle.err_code(), handle.err_msg())
    }
}

impl<'a,'r> From<&'a archive::Handle<'r>> for ArchiveResult<()> {
    fn from(handle: &'a archive::Handle) -> ArchiveResult<()> {
        match handle.err_code() {
            ErrCode(0) => Ok(()),
            _ => Err(ArchiveError::from(handle)),
        }
    }
}
