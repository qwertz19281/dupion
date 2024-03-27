use dupion::{dedup::{btrfs::BtrfsDedup, Deduper}, driver::{platterwalker::PlatterWalker, uringer::Uringer, Driver}, opts::Opts, output::{groups::print_groups, tree::print_tree, treediff::print_treediff}, phase::Phase, print_statw, process::{calculate_dir_hash, export, find_shadowed}, stat_section_end, stat_section_start, state::State, util::*, vfs::VfsId, zip::setlocale_hack};
use std::{io::{stderr, IsTerminal as _}, path::PathBuf, sync::atomic::Ordering, time::Duration};
use parking_lot::RwLock;
use clap::{Parser, ValueEnum};

fn main() {
    setlocale_hack();

    DISP_ANSI.store(stderr().is_terminal(), Ordering::Relaxed);

    let o = OptInput::parse();

    let euid = unsafe {libc::geteuid()};

    let opts = Box::leak(Box::new(Opts{
        paths: o.dirs.clone(),
        cache_path: o.cache_path.clone(),
        verbose: o.verbose,
        shadow_rule: o.shadow_rule,
        force_absolute_paths: o.absolute,
        read_buffer: o.read_buffer*1024*1024,
        max_open_files: o.max_open_files,
        prefetch_budget: o.prefetch_budget*1024*1024,
        dedup_budget: o.dedup_budget*1024*1024,
        cache_dropbehind: o.cache_dropbehind,
        pass_1_hash: o.pass_1_hash,
        archive_cache_mem: o.archive_cache_mem*1024*1024,
        dir_prefetch: o.dir_prefetch,
        read_archives: o.read_archives,
        //huge_zip_thres: ((o.huge_zip_thres * 1048576.0) as usize +1024)/4096*4096,
        threads: o.threads,
        scan_size_min: if o.min_size.is_empty() {0} else {parse_str_prefix(&o.min_size)},
        scan_size_max: if o.max_size.is_empty() {u64::MAX} else {parse_str_prefix(&o.max_size)},
        aggressive_dedup: false,
        dedup_simulate: o.dedup_simulate,
        fiemap: o.fiemap,
        skip_no_phys: o.phys_only && o.fiemap != 0,
        euid,
    }));

    if opts.paths.is_empty() {
        opts.paths = vec![std::env::current_dir().unwrap()];
    }
    if opts.threads == 0 {
        opts.threads = get_threads();
    }

    // dedup engine requires phys
    if opts.fiemap == 0 && matches!(o.dedup,Some(DedupMode::Btrfs)) {
        opts.fiemap = 1;
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

    if let Some(DedupMode::Btrfs) = o.dedup {
        eprintln!("\n#### Dedup\n");
        stat_section_start();
        BtrfsDedup{}.dedup(state,opts).unwrap();
        stat_section_end();
    }

    let mut state = state.write();

    if matches!(o.output, OutputMode::Disabled) {return;}

    eprintln!("\n#### Calculate");
    
    assert!(!state.tree.entries.is_empty(),"No Duplicates found");

    let _ = calculate_dir_hash(&mut state,VfsId::ROOT);
    find_shadowed(&mut state,VfsId::ROOT);

    eprintln!("#### Sort");

    let sorted = export(&mut state);

    eprintln!("#### Result");

    match o.output {
        OutputMode::Groups => print_groups(&sorted, &state, opts),
        OutputMode::Tree => print_tree(&state, opts),
        OutputMode::Diff => print_treediff(&mut state, opts),
        OutputMode::Disabled => {}, //TODO exit before calc and sort
    }
}

pub fn scan(o: &OptInput, opts: &'static Opts, state: &'static RwLock<State>) {
    let mut d = Uringer::new(opts);

    eprintln!("\n#### Pass 1\n");

    stat_section_start();
    spawn_info_thread(opts);
    d.run(state,opts,Phase::Size).unwrap();
    stat_section_end();

    if o.bench_pass_1 {return;}

    eprintln!("\n#### Pass 2\n");

    stat_section_start();
    d.run(state,opts,Phase::Hash).unwrap();
    stat_section_end();

    eprintln!("\n#### Pass 3\n");

    stat_section_start();
    d.run(state,opts,Phase::PostHash).unwrap();
    stat_section_end();

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
    if DISP_ANSI.load(Ordering::Relaxed) && !o.verbose {
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
                    let mut err = std::io::stderr().lock();
                    print_statw(&mut err, true);
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
        _ => return std::thread::available_parallelism().map(Into::into).unwrap_or(1),
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

    /// File lower size limit for scanning in bytes (supports IEC prefix)  
    /// Note that skipping (hashing of) files will affect finding of duplicate files and folders, as a folder can only be hashed if all contained children were hashed
    #[arg(long, default_value = "0", verbatim_doc_comment)]
    pub min_size: String,
    /// File upper size limit for scanning in bytes (supports IEC prefix)  
    /// Note that skipping (hashing of) files will affect finding of duplicate files and folders, as a folder can only be hashed if all contained children were hashed
    #[arg(long, default_value = "", verbatim_doc_comment)]
    pub max_size: String,

    /// Also search inside archives. requires to scan and hash every archive  
    /// Not supported yet with new uring engine
    #[arg(short='a', long)]
    pub read_archives: bool, //TODO: build mode w/o archive support

    /// Deduplication mode (-/btrfs). Disabled by default  
    /// btrfs: Use ioctl_file_dedupe_range on supported filesystems
    #[arg(long, verbatim_doc_comment)]
    pub dedup: Option<DedupMode>,
    /// EXPERIMENTAL Dedup even if first extent match. Currently this would dedup everything, even if already deduped. N/A with uring engine
    #[arg(long)]
    pub aggressive_dedup: bool,
    /// Simulate if dedup enabled
    #[arg(long)]
    pub dedup_simulate: bool,

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
    #[arg(long, default_value_t = 1)]
    pub read_buffer: usize,
    /// EXPERIMENTAL Max open files
    #[arg(long, default_value_t = 256)]
    pub max_open_files: u64,
    /// EXPERIMENTAL Prefetch budget in MiB
    #[arg(long, default_value_t = 256)]
    pub prefetch_budget: u64,
    /// EXPERIMENTAL Dedup budget in MiB
    #[arg(long, default_value_t = 512)]
    pub dedup_budget: u64,
    /// EXPERIMENTAL Control FIEMAP mode. 0 = disable, >1 = enable and set FIEMAP-hash limit
    #[arg(long, default_value_t = 1024)]
    pub fiemap: usize,
    /// EXPERIMENTAL Skip hashing of files without a physical location (File too small or too many fragments)  
    /// Should only be used in dedup-only use, as finding of duplicate files/folders is severely impacted
    #[arg(long, verbatim_doc_comment)]
    pub phys_only: bool,
    /// Threaded archive read cache limit in MiB
    #[arg(long, default_value_t = 1024)]
    pub archive_cache_mem: usize,
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
    /// Should not be set if the cache previously only ran with --phys-only (except for dedup-only use)
    #[arg(long)]
    pub no_scan: bool,

    /// Verbose
    #[arg(short, long)]
    pub verbose: bool,
    
    /// Directories to scan. cwd if none defined
    #[arg()]
    pub dirs: Vec<PathBuf>,
}

#[derive(ValueEnum, Clone)]
pub enum OutputMode {
    #[value(alias="g")]
    Groups,
    #[value(alias="t")]
    Tree,
    #[value(alias="d")]
    Diff,
    #[value(alias="-")]
    Disabled,
}

#[derive(ValueEnum, Clone)]
pub enum DedupMode {
    Btrfs
}

fn parse_str_prefix(v: &str) -> u64 {
    let v = v.to_ascii_lowercase();
    if !v.is_ascii() {
        panic!("min/max_size must be number and optionally prefix");
    }
    let (num,prefix) = v.split_at(v.len()-1);
    assert!(prefix.len() == 1);
    let prefix = prefix.chars().next().unwrap();
    if prefix.is_ascii_digit() {
        return v.parse().unwrap();
    }
    let num = num.parse::<u128>().unwrap();
    let shift = match prefix {
        'k' => 10, 'm' => 20, 'g' => 30, 't' => 40, 'p' => 50, 'e' => 60, 'z' => 70, 'y' => 80,
        _ => panic!("Invalid IEC prefix, must be k/M/G/T/P/E/Z/Y")
    };
    (num << shift).try_into().unwrap()
}
