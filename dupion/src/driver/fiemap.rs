use std::fmt::Debug;
use std::num::NonZeroUsize;
use std::os::fd::AsRawFd;
use std::path::Path;
use std::sync::Arc;

use fiemap::FiemapExtentFlags;

use crate::util::Hash;

#[derive(Clone)]
pub struct FiemapInfo {
    /// First phys
    pub phys: u64,
    /// Will be incomplete if scan_whole=false
    /// 
    /// Raw extent count, not the merged one! do not compare on this, compare on fiemap_hash
    pub n_extents: usize,
    /// Will be incomplete if scan_whole=false
    /// 
    /// How many of n_extents are shared
    pub n_extents_shared: usize,
    /// Only if all non-empty extents have phys and not inlined
    /// 
    /// If a fiemap_hash of two files matches, then we assume that these files are dups
    pub fiemap_hash: Option<Hash>,
}

// TODO fiemap::fm_length is garbage (2^64−1)
pub fn read_fiemap(fd: &impl AsRawFd, fiemap: bool, scan_whole: bool, hash: bool, max_e: usize) -> Result<Option<FiemapInfo>,ReadFiemapError> {
    pub fn legal_flags() -> FiemapExtentFlags {
        FiemapExtentFlags::LAST |
        FiemapExtentFlags::ENCODED |
        FiemapExtentFlags::DATA_ENCRYPTED |
        FiemapExtentFlags::NOT_ALIGNED |
        FiemapExtentFlags::DATA_TAIL |
        FiemapExtentFlags::UNWRITTEN |
        FiemapExtentFlags::MERGED |
        FiemapExtentFlags::SHARED
    }

    if !fiemap {return Ok(None);}
    if !hash {
        let mut result = None;
        let mut n_extents = 0;
        let mut n_extents_shared = 0;
        for e in fiemap::fiemap2(fd)? {
            let e = e?;

            if n_extents > max_e || e.fm_extent_count as usize > max_e { // TODO don't. fm_extent_count is garbage too
                return Err(ReadFiemapError::ExtentLimitExceeded);
            }
            if (!legal_flags()).intersects(e.fe_flags) {
                return Ok(None);
            }

            if result.is_none() && !e.fe_flags.intersects(FiemapExtentFlags::DATA_INLINE) && e.fe_physical != 0 {
                result = Some(e.fe_physical);
            }

            n_extents += 1;
            if e.fe_flags.intersects(FiemapExtentFlags::SHARED) {
                n_extents_shared += 1;
            }

            if result.is_some() && !scan_whole {
                return Ok(result.map(|phys| FiemapInfo { phys, n_extents, n_extents_shared, fiemap_hash: None } ));
            }
        }
        return Ok(result.map(|phys| FiemapInfo { phys, n_extents, n_extents_shared, fiemap_hash: None } ));
    }

    let mut hasher = blake3::Hasher::new();

    let mut build_off = 0;
    let mut build_len = 0;
    let mut build_phys = 0;

    let mut first_phys = None;

    let mut n_extents = 0;
    let mut n_extents_shared = 0;
    for e in fiemap::fiemap2(fd)? {
        let e = e?;
        
        if n_extents > max_e || e.fm_extent_count as usize > max_e {
            return Err(ReadFiemapError::ExtentLimitExceeded);
        }
        if (!legal_flags()).intersects(e.fe_flags) {
            return Ok(None);
        }

        let mut current_phys = 0;
        if !e.fe_flags.intersects(FiemapExtentFlags::DATA_INLINE) {
            current_phys = e.fe_physical;
        }
        if first_phys.is_none() && current_phys != 0 {
            first_phys = Some(current_phys);
        }

        let current_empty = e.fe_flags.intersects(FiemapExtentFlags::UNWRITTEN | FiemapExtentFlags::DATA_TAIL);

        if current_empty {
            continue;
        }

        if current_phys == 0 {
            return Ok(None);
        }

        if (build_off + build_len == e.fe_logical) && (build_phys + build_len == e.fe_physical) {
            // next extent perfectly extends
            //eprintln!("YESM");
            build_len += e.fe_length;
        } else {
            if build_off != 0 || build_len != 0 || build_phys != 0 {
                hasher.update(&build_off.to_le_bytes());
                hasher.update(&build_len.to_le_bytes());
                hasher.update(&build_phys.to_le_bytes());
            }
            build_off = e.fe_logical;
            build_len = e.fe_length;
            build_phys = e.fe_physical;
        }

        //dbg!(build_off,build_len,build_phys,e);

        n_extents += 1;
        if e.fe_flags.intersects(FiemapExtentFlags::SHARED) {
            n_extents_shared += 1;
        }
    }

    if first_phys.is_none() {
        return Ok(None);
    }

    hasher.update(&build_off.to_le_bytes());
    hasher.update(&build_len.to_le_bytes());
    hasher.update(&build_phys.to_le_bytes());

    let hash = hasher.finalize();

    Ok(Some(FiemapInfo {
        phys: first_phys.unwrap(),
        n_extents,
        n_extents_shared,
        fiemap_hash: Some(Arc::new(hash.into())),
    }))
}

use thiserror::Error;

#[derive(Error, Debug)]
pub enum ReadFiemapError {
    #[error("IO Error: ")]
    Io(#[from] std::io::Error),
    #[error("Extent limit exceeded")]
    ExtentLimitExceeded,
}

impl Debug for FiemapInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FiemapInfo")
            .field("phys", &self.phys)
            .field("n_extents", &self.n_extents)
            .field("n_extents_shared", &self.n_extents_shared)
            .field("fiemap_hash", &self.fiemap_hash.as_deref().map(hex::encode))
            .finish()
    }
}

