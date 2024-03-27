use std::path::Path;
use std::sync::atomic::Ordering;

use crate::opts::Opts;
use crate::state::State;
use crate::util::{DISP_FOUND_BYTES, DISP_FOUND_FILES};
use crate::vfs::VfsId;

pub fn size_file(path: &Path, size: u64, ctime: i64, uid: u32, phy_off: Option<u64>, n_extents: Option<usize>, s: &mut State, opts: &Opts) -> VfsId {
    opts.log_verbosed("SIZE", path);

    DISP_FOUND_BYTES.fetch_add(size,Ordering::Relaxed);
    DISP_FOUND_FILES.fetch_add(1,Ordering::Relaxed);

    let id = s.tree.cid_and_create(path);
    s.validate(id,ctime,Some(size),None);

    let e = &mut s.tree[id];
    e.is_file = true;
    
    e.file_size = Some(size);
    e.uid = Some(uid);
    e.phys = phy_off;
    e.n_extents = n_extents;
    
    s.push_to_size_group(id,true,false).unwrap();
    if s.tree[id].file_hash.is_some() {
        s.push_to_hash_group(id,true,false).unwrap();
        //disp_processed_bytes.fetch_add(size as usize,Ordering::Relaxed);
        //disp_processed_files.fetch_add(1,Ordering::Relaxed);
    }

    if s.is_file_read_candidate(id,opts) {
        s.tree[id].disp_add_relevant();
    }

    id
}
