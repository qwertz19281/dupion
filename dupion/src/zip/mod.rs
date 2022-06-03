use super::*;
use opts::Opts;
use std::{ffi::CString, sync::Arc, path::Path, io::{Read, Write, Seek}};
use parking_lot::RwLock;
use state::State;
use libarchive::{entry::OwnedEntry, reader::{StreamReader, Reader as AReader, Builder}, archive::{FileType, Entry, ReadFormat, ReadFilter, ReadCompression}};

pub fn decode_zip<'r,R>(mut ar: R, zip_path: &Path, state: &RwLock<State>, opts: &Opts) -> AnyhowResult<()> where R: AReader<'r> {
    let result = (||{
        let mut buf = [0u8;8192];

        let mut error_counter = 0usize;

        'z: loop {
            let mut e = OwnedEntry::new().unwrap();

            
                if !try_counted!(ar.next_header2(&mut e),error_counter,'z,"\tError reading ZIP header: {} ({})",opts.path_disp(&zip_path)) {
                    break;
                }

                let name = e.pathname();
                let size = e.size();
                let filetype = e.filetype();

                if filetype == FileType::RegularFile {
                    if let Some(name) = name {

                        opts.log_verbosed("HASH", &zip_path.join(name));
                        
                        let mut hasher = blake3::Hasher::new();

                        let mut r2 = 0;

                        loop{
                            let r = try_counted!(ar.read(&mut buf),error_counter,'z,"\tError reading zipped data: {} ({})",opts.path_disp(&zip_path.join(name)));
                            if r == 0 {
                                break;
                            }
                            hasher.write(&buf[..r]).unwrap();
                            r2 += r;
                        }

                        if (r2 as i64) < size {
                            eprintln!("\tWARN: assertion failed: r2 as i64 >= size ({})",opts.path_disp(&zip_path.join(name)));
                            //continue;
                        }

                        let hash = Arc::new(hasher.finalize().into());

                        let path = zip_path.join(name);

                        let mut s = state.write();

                        let id = s.tree.cid_and_create(&path);

                        let e = &mut s.tree[id];
                        assert!(!e.is_dir);
                        e.is_dir = false; //TODO implement nested archive search (big todo)
                        e.is_file = true;
                        e.file_size = Some(r2 as u64);
                        e.file_hash = Some(hash);
                        e.phys = None;
                        e.valid = true;
                        //e.dir_size = None;
                        //e.dir_hash = None;
                        //e.childs = Vec::new();

                        s.push_to_size_group(id,true,false).unwrap();
                        s.push_to_hash_group(id,true,false).unwrap();
                    };
                }

                error_counter = 0;
        }

        /*let mut state = state.write().unwrap();
        if let Some(id) = state.tree.cid(zip_path) {
            state.set_valid(id);
        }*/

        Ok(())
    })();
    if result.is_err() {
        eprintln!("\tUpgrade(1) {}",opts.path_disp(zip_path));
        if let Some(e) = state.write().tree.resolve_mut(zip_path) {
            e.failure = Some(1);
        }
    }
    result
}

pub fn open_zip<'r,R>(r: R,path: &Path,state: &RwLock<State>, opts: &Opts) -> AnyhowResult<StreamReader<'r,R>> where R: Read+Seek {
    let result = (||{
        let mut b = Builder::new();

        b.support_format(ReadFormat::All)?;
        b.support_filter(ReadFilter::All)?;
        b.support_compression(ReadCompression::All)?;

        Ok(b.open_seekable_stream(r)?)
    })();
    if result.is_err() {
        eprintln!("\tUpgrade(1) {}",opts.path_disp(path));
        if let Some(e) = state.write().tree.resolve_mut(path) {
            e.failure = Some(1);
        }
    }
    result
}

///libarchive requires this
pub fn setlocale_hack() {
    unsafe {
        let s = CString::new(Vec::new()).unwrap();
        libc::setlocale(libc::LC_ALL,s.as_ptr());
    }
}

#[macro_export]
macro_rules! try_counted {
    ($oof:expr,$ec:ident,$ll:lifetime,$fmt:expr,$($args:tt)*) => {
        match $oof {
            Ok(f) => {
                f
            },
            Err(e) => {
                $ec += 1;
                eprintln!($fmt,e,$($args)*);
                if $ec >= 16 {
                    return Err(e.into());
                }
                continue $ll
            },
        }
    };
}
