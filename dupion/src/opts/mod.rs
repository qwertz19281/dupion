use super::*;
use std::path::{Path, PathBuf};
use vfs::is_absolute;

pub struct Opts {
    pub paths: Vec<PathBuf>,
    pub verbose: bool,
    pub shadow_rule: u8,
    pub force_absolute_paths: bool,
    pub read_buffer: usize,
    pub prefetch_budget: u64,
    //pub huge_zip_thres: u64,
    pub threads: usize,
    pub pass_1_hash: bool,
    pub archive_cache_mem: usize,
    pub dir_prefetch: bool,
    pub read_archives: bool,
    pub scan_size_min: u64,
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
            eprintln!("\t{} {}",prefix,s);
        }
    }
    pub fn path_disp<'a>(&self, path: &'a Path) -> &'a str {
        if !self.force_absolute_paths && self.paths.len() == 1 {
            path.strip_prefix(&self.paths[0]).unwrap_or(path).to_str().unwrap()
        }else{
            path.to_str().unwrap()
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
}