use self::util::get_rlimit;

use super::*;
use std::borrow::Cow;
use std::path::{Path, PathBuf};
use vfs::is_absolute;

pub struct Opts {
    pub paths: Vec<PathBuf>,
    pub cache_path: PathBuf,
    pub verbose: bool,
    pub shadow_rule: u8,
    pub force_absolute_paths: bool,
    pub read_buffer: usize,
    pub cache_dropbehind: bool,
    pub prefetch_budget: u64,
    pub max_open_files: u64,
    pub dedup_budget: u64,
    //pub huge_zip_thres: u64,
    pub threads: usize,
    pub pass_1_hash: bool,
    pub archive_cache_mem: usize,
    pub dir_prefetch: bool,
    pub read_archives: bool,
    pub scan_size_min: u64,
    pub scan_size_max: u64,
    pub aggressive_dedup: bool,
    pub dedup_simulate: bool,
    pub fiemap: usize,
    pub skip_no_phys: bool,
    pub euid: u32,
}

impl Opts {
    pub fn validate(&mut self) -> AnyhowResult<()> {
        assert!(self.shadow_rule < 4, "show_shadow must be in range 0-3");
        for p in &mut self.paths {
            assert!(p.is_dir() || p.is_file(),"Passed directories must be existing directories");
            *p = p.canonicalize()?;
            is_absolute(p);
            assert!(p.is_dir() || p.is_file());
        }
        Ok(())
    }
    pub fn log_verbosed(&self, prefix: &str, path: &Path) {
        if self.verbose {
            let s = self.path_disp(path);
            dprintln!("\t{} {}",prefix,s);
        }
    }
    pub fn path_disp<'a>(&self, path: &'a Path) -> Cow<'a,str> {
        if !self.force_absolute_paths && self.paths.len() == 1 {
            path.strip_prefix(&self.paths[0]).unwrap_or(path).to_string_lossy()
        }else{
            path.to_string_lossy()
        }
    }

    pub fn zip_by_extension(&self, p: &Path) -> bool {
        if !self.read_archives {return false;}
        let s = p.to_string_lossy();
        s.ends_with(".zip") ||
        s.ends_with(".tar") ||
        //s.ends_with(".rar") || libarchive oofs on rars
        s.ends_with(".7z") ||
        s.ends_with(".tar.gz") ||
        s.ends_with(".tar.xz") ||
        false
    }

    pub(crate) fn limit_open_files(&self, sub: usize, min: usize, max: usize) -> usize {
        let (cur,_) = get_rlimit();
        (cur as usize).saturating_sub(16).saturating_sub(sub).clamp(min, max)
    }
}
