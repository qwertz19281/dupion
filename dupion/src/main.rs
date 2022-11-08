use dupion::{state::State, opts::Opts, driver::{Driver, platterwalker::PlatterWalker}, phase::Phase, process::{export, calculate_dir_hash, find_shadowed}, util::*, vfs::VfsId, zip::setlocale_hack, output::{tree::print_tree, groups::print_groups, treediff::print_treediff}, dedup::{Deduper, btrfs::BtrfsDedup}, print_stat};
use std::{time::Duration, sync::{atomic::Ordering}, path::PathBuf};
use parking_lot::RwLock;
use clap::Parser;

use dupion::dprintln;

fn main() {
    setlocale_hack();

    let o = OptInput::parse();

    let opts = Box::leak(Box::new(Opts{
        paths: o.dirs.clone(),
        cache_path: o.cache_path.clone(),
        verbose: o.verbose,
        shadow_rule: o.shadow_rule,
        force_absolute_paths: o.absolute,
        read_buffer:       ((o.read_buffer * 1048576.0) as usize +1024)/4096*4096,
        prefetch_budget: ((o.prefetch_budget * 1048576.0) as u64 +1024)/4096*4096,
        dedup_budget: ((o.dedup_budget * 1048576.0) as u64 +1024)/4096*4096,
        cache_dropbehind: o.cache_dropbehind,
        pass_1_hash: o.pass_1_hash,
        archive_cache_mem: ((o.archive_cache_mem * 1048576.0) as usize +1024)/4096*4096,
        dir_prefetch: o.dir_prefetch,
        read_archives: o.read_archives,
        //huge_zip_thres: ((o.huge_zip_thres * 1048576.0) as usize +1024)/4096*4096,
        threads: o.threads,
        scan_size_min: o.min_size,
        scan_size_max: o.max_size,
        aggressive_dedup: o.aggressive_dedup,
    }));

    if opts.paths.is_empty() {
        opts.paths = vec![std::env::current_dir().unwrap()];
    }
    if opts.threads == 0 {
        opts.threads = get_threads();
    }

    opts.validate().unwrap();

    let state = Box::leak(Box::new(RwLock::new(State::new(!o.no_cache))));

    if !o.bench_pass_1 {
        state.write().eventually_load_vfs(&opts.cache_path);
    }

    if !o.no_scan {
        scan(&o, opts, state);
    }else{
        dirty_load(&o, opts, state);
    }

    if o.bench_pass_1 {return;}

    if o.dedup == "btrfs" {
        dprintln!("\n\n#### Dedup");
        DISP_ENABLED.store(true, Ordering::Relaxed);
        BtrfsDedup{}.dedup(state,opts).unwrap();
        DISP_ENABLED.store(false, Ordering::Relaxed);
        print_stat();
    }

    let mut state = state.write();

    if matches!(o.output, OutputMode::Disabled) {return;}

    dprintln!("\n\n#### Calculate");
    
    assert!(!state.tree.entries.is_empty(),"No Duplicates found");

    let _ = calculate_dir_hash(&mut state,VfsId::ROOT);
    find_shadowed(&mut state,VfsId::ROOT);

    dprintln!("#### Sort");

    let sorted = export(&mut state);

    dprintln!("#### Result");

    match o.output {
        OutputMode::Groups => print_groups(&sorted, &state, opts),
        OutputMode::Tree => print_tree(&state, opts),
        OutputMode::Diff => print_treediff(&mut state, opts),
        OutputMode::Disabled => {}, //TODO exit before calc and sort
    }
}

pub fn scan(o: &OptInput, opts: &'static Opts, state: &'static RwLock<State>) {
    let mut d = PlatterWalker::new();

    dprintln!("\n#### Pass 1\n");

    DISP_ENABLED.store(true, Ordering::Relaxed);
    spawn_info_thread(opts);
    d.run(state,opts,Phase::Size).unwrap();
    DISP_ENABLED.store(false, Ordering::Relaxed);

    print_stat();

    if o.bench_pass_1 {return;}

    dprintln!("\n\n#### Pass 2\n");

    DISP_ENABLED.store(true, Ordering::Relaxed);
    d.run(state,opts,Phase::Hash).unwrap();
    DISP_ENABLED.store(false, Ordering::Relaxed);
    print_stat();

    dprintln!("\n\n#### Pass 3\n");

    DISP_ENABLED.store(true, Ordering::Relaxed);
    d.run(state,opts,Phase::PostHash).unwrap();
    DISP_ENABLED.store(false, Ordering::Relaxed);
    print_stat();

    let mut state = state.write();

    state.eventually_store_vfs(&opts.cache_path, true);
}

pub fn dirty_load(o: &OptInput, opts: &'static Opts, state: &'static RwLock<State>) {
    let mut state = state.write();

    for root in &opts.paths {
        let id = state.tree.cid_and_create(root);
        state.set_valid(id);
    }
}

pub fn spawn_info_thread(o: &Opts) {
    if !o.verbose {
        std::thread::spawn(move || {
            let mut note = 0usize;
            loop {
                std::thread::sleep(Duration::from_millis(500));
                note+=1;
                if note >= 1200 {
                    note = 0;
                    VFS_STORE_NOTIF.store(true, Ordering::Relaxed);
                }
                if DISP_ENABLED.load(Ordering::Relaxed) {
                    print_stat();
                }
            }
        });
    }
}


pub fn get_threads() -> usize {
    match std::env::var("RAYON_NUM_THREADS")
        .ok()
        .and_then(|s| s.parse().ok())
    {
        Some(x) if x > 0 => return x,
        Some(x) if x == 0 => return num_cpus::get(),
        _ => {}
    }

    // Support for deprecated `RAYON_RS_NUM_CPUS`.
    match std::env::var("RAYON_RS_NUM_CPUS")
        .ok()
        .and_then(|s| s.parse().ok())
    {
        Some(x) if x > 0 => x,
        _ => num_cpus::get(),
    }
}

#[derive(Parser)]
#[clap(version, about)]
pub struct OptInput {
    /// Results output mode (g/t/d/-), what type of result should be printed
    /// groups: duplicate entries in sorted size groups
    /// tree: json as tree
    /// diff: like tree, but exact dir comparision, reveals diffs and supersets
    /// -: disabled
    #[arg(short, long, default_value = "g", verbatim_doc_comment)]
    pub output: OutputMode,

    /// Set how files/directory should be hidden/omitted (shadowed are e.g. childs of duplicate dirs) (0-3)
    /// 0: show ALL, including pure shadowed groups
    /// 1: show all except pure shadowed groups
    /// 2: show shadowed only if there is also one non-shadowed in the group
    /// 3: never show shadowed
    #[arg(short, long, default_value_t = 2, verbatim_doc_comment)]
    pub shadow_rule: u8,
    
    /// Force to display absolute paths
    #[arg(long)]
    pub absolute: bool,

    /// File lower size limit for scanning in bytes  
    #[arg(long, default_value_t = 0)]
    pub min_size: u64,
    /// File upper size limit for scanning in bytes  
    #[arg(long, default_value_t = u64::MAX)]
    pub max_size: u64, //TODO parse K/M/G prefixes

    /// Also search inside archives. requires to scan and hash every archive
    #[arg(short='a', long)]
    pub read_archives: bool, //TODO: build mode w/o archive support

    /// EXPERIMENTAL Deduplication mode (-/btrfs). Disabled by default
    #[arg(long, default_value = "")]
    pub dedup: String,

    /// EXPERIMENTAL Dedup even if first extent match. Currently this would dedup everything, even if already deduped
    #[arg(long)]
    pub aggressive_dedup: bool,

    /// Path of dupion cache
    #[arg(long, default_value = "./dupion_cache")]
    pub cache_path: PathBuf,
    /// Don't read or write cache file
    #[arg(long)]
    pub no_cache: bool,
    
    /// Number of threads for zip decoding, 0 = RAYON_NUM_THREADS or num_cpu logical count
    #[arg(short, long, default_value_t = 0)]
    pub threads: usize,

    /// EXPERIMENTAL Read buffer in MiB
    #[arg(long, default_value_t = 1.0)]
    pub read_buffer: f64,
    /// EXPERIMENTAL Prefetch budget in MiB
    #[arg(long, default_value_t = 32.0)]
    pub prefetch_budget: f64,
    /// EXPERIMENTAL Dedup budget in MiB
    #[arg(long, default_value_t = 256.0)]
    pub dedup_budget: f64,
    /// Threaded archive read cache limit in MiB
    #[arg(long, default_value_t = 1024.0)]
    pub archive_cache_mem: f64,
    /// Enable cache dropbehind to reduce cache pressure in hash scan. Can affect performance positively or negatively
    #[arg(long)]
    pub cache_dropbehind: bool,
    /// EXPERIMENTAL Enable hashing in 1st pass. Can affect performance positively or negatively
    #[arg(long)]
    pub pass_1_hash: bool,
    /// Abort after pass 1
    #[arg(long)]
    pub bench_pass_1: bool,
    /// EXPERIMENTAL Prefetch directory metadata, eventually fails on non-root
    #[arg(long)]
    pub dir_prefetch: bool,
    /// EXPERIMENTAL Don't scan for files, use found files from cache instead
    #[arg(long)]
    pub no_scan: bool,

    /// Verbose
    #[arg(short, long)]
    pub verbose: bool,
    

    /// Directories to scan. cwd if none defined
    #[arg()]
    pub dirs: Vec<PathBuf>,
}

#[derive(Clone)]
pub enum OutputMode {
    Groups,
    Tree,
    Diff,
    Disabled,
}

impl From<&str> for OutputMode {
    fn from(s: &str) -> Self {
        match s.chars().next().map(|c| c.to_ascii_lowercase() ) {
            Some('g') => Self::Groups,
            Some('t') => Self::Tree,
            Some('d') => Self::Diff,
            Some('-') => Self::Disabled,
            _ => panic!("Invalid output mode"),
        }
    }
}
