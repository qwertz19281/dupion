use dupion::{state::State, opts::Opts, driver::{Driver, platterwalker::PlatterWalker}, phase::Phase, process::{export, calculate_dir_hash, find_shadowed}, util::*, vfs::VfsId, zip::setlocale_hack, output::{tree::print_tree, groups::print_groups, treediff::print_treediff}, dedup::{Deduper, btrfs::BtrfsDedup}};
use std::{time::Duration, sync::{atomic::Ordering}, path::PathBuf, io::Write};
use size_format::SizeFormatterBinary;
use parking_lot::RwLock;
use structopt::*;

fn main() {
    setlocale_hack();

    let o = OptInput::from_args();

    let opts = Box::leak(Box::new(Opts{
        paths: o.dirs.clone(),
        verbose: o.verbose,
        shadow_rule: o.shadow_rule,
        force_absolute_paths: o.absolute,
        read_buffer:       ((o.read_buffer * 1048576.0) as usize +1024)/4096*4096,
        prefetch_budget: ((o.prefetch_budget * 1048576.0) as u64 +1024)/4096*4096,
        pass_1_hash: o.pass_1_hash,
        archive_cache_mem: ((o.archive_cache_mem * 1048576.0) as usize +1024)/4096*4096,
        dir_prefetch: o.dir_prefetch,
        read_archives: o.read_archives,
        //huge_zip_thres: ((o.huge_zip_thres * 1048576.0) as usize +1024)/4096*4096,
        threads: o.threads,
        scan_size_min: o.min_size,
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
        state.write().eventually_load_vfs();
    }

    if !o.no_scan {
        scan(&o, opts, state);
    }else{
        dirty_load(&o, opts, state);
    }

    if o.dedup == "btrfs" {
        eprintln!("\n\n#### Dedup");
        disp_enabled.store(true, Ordering::Release);
        BtrfsDedup{}.dedup(state,opts).unwrap();
        disp_enabled.store(false, Ordering::Release);
        print_stat();
    }

    let mut state = state.write();

    eprintln!("\n\n#### Calculate");
    
    assert!(!state.tree.entries.is_empty(),"No Duplicates found");

    let _ = calculate_dir_hash(&mut state,VfsId::ROOT);
    find_shadowed(&mut state,VfsId::ROOT);

    eprintln!("#### Sort");

    let sorted = export(&mut state);

    eprintln!("#### Result");

    match o.output {
        OutputMode::Groups => print_groups(&sorted, &state, &opts),
        OutputMode::Tree => print_tree(&mut state, &opts),
        OutputMode::Diff => print_treediff(&mut state, &opts),
    }
}

pub fn scan(o: &OptInput, opts: &'static Opts, state: &'static RwLock<State>) {
    let mut d = PlatterWalker::new();

    eprintln!("\n#### Pass 1\n");

    disp_enabled.store(true, Ordering::Release);
    spawn_info_thread(&opts);
    d.run(state,opts,Phase::Size).unwrap();
    disp_enabled.store(false, Ordering::Release);

    print_stat();

    if o.bench_pass_1 {return;}

    eprintln!("\n\n#### Pass 2\n");

    disp_enabled.store(true, Ordering::Release);
    d.run(state,opts,Phase::Hash).unwrap();
    disp_enabled.store(false, Ordering::Release);
    print_stat();

    eprintln!("\n\n#### Pass 3\n");

    disp_enabled.store(true, Ordering::Release);
    d.run(state,opts,Phase::PostHash).unwrap();
    disp_enabled.store(false, Ordering::Release);
    print_stat();

    let mut state = state.write();

    state.eventually_store_vfs(true);
}

pub fn dirty_load(o: &OptInput, opts: &'static Opts, state: &'static RwLock<State>) {
    let mut state = state.write();

    for root in &opts.paths {
        let id = state.tree.cid_and_create(root);
        state.set_valid(id);
    }

    if o.bench_pass_1 {return;}
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
                    vfs_store_notif.store(true, Ordering::Release);
                }
                if disp_enabled.load(Ordering::Acquire) {
                    print_stat();
                }
            }
        });
    }
}

pub fn print_stat() {
    let processed_files = disp_processed_files.load(Ordering::Acquire);
    let relevant_files = disp_relevant_files.load(Ordering::Acquire);
    let found_files = disp_found_files.load(Ordering::Acquire);
    let processed_bytes = disp_processed_bytes.load(Ordering::Acquire);
    let relevant_bytes = disp_relevant_bytes.load(Ordering::Acquire);
    let found_bytes = disp_found_bytes.load(Ordering::Acquire);
    let prev_bytes = disp_prev.swap(processed_bytes, Ordering::AcqRel);
    let deduped_bytes = disp_deduped_bytes.load(Ordering::Acquire);
    let alloced = alloc_mon.load(Ordering::Acquire) as u64;
    assert!(processed_bytes >= prev_bytes);

    if deduped_bytes == u64::MAX {
        eprint!(
            //"\x1B[2K\rAnalyzed files: {:>filefill$}/{} bytes: {:>12}B/{}B ({:>12}B/s) percent: {}%",
            "\x1B[2K\rFound: {} ({}B)        Hashed: {}/{} {}B/{}B ({}B/s)        alloc={}B",
            found_files,
            SizeFormatterBinary::new(found_bytes),
            processed_files,
            relevant_files,
            SizeFormatterBinary::new(processed_bytes),
            SizeFormatterBinary::new(relevant_bytes),
            SizeFormatterBinary::new((processed_bytes - prev_bytes)*2),
            SizeFormatterBinary::new(alloced),
            //( processed_bytes as f32 / relevant_bytes as f32 )*100.0,
            //filefill = relevant_files.to_string().len(),
        );
    }else{
        eprint!(
            //"\x1B[2K\rAnalyzed files: {:>filefill$}/{} bytes: {:>12}B/{}B ({:>12}B/s) percent: {}%",
            "\x1B[2K\rDeduplication: Processed: {}/{} {}B/{}B ({}B/s)        Deduped: {}B",
            processed_files,
            relevant_files,
            SizeFormatterBinary::new(processed_bytes),
            SizeFormatterBinary::new(relevant_bytes),
            SizeFormatterBinary::new((processed_bytes - prev_bytes)*2),
            SizeFormatterBinary::new(deduped_bytes as u64),
            //( processed_bytes as f32 / relevant_bytes as f32 )*100.0,
            //filefill = relevant_files.to_string().len(),
        );
    }
    let _ = std::io::stdout().flush();
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

#[derive(StructOpt)]
#[structopt(name = "dupion", about = "Find duplicate files and folders")]
pub struct OptInput {
    #[structopt(long,default_value="1.0",help="EXPERIMENTAL read buffer in MiB")]
    pub read_buffer: f64,
    #[structopt(long,default_value="16.0",help="EXPERIMENTAL prefetch budget in MiB")]
    pub prefetch_budget: f64,
    #[structopt(long,default_value="1024.0",help="threaded archive read cache limit in MiB")]
    pub archive_cache_mem: f64,
    #[structopt(short,long,default_value="0",help="number of threads for zip decoding, 0 = RAYON_NUM_THREADS or num_cpu logical count")]
    pub threads: usize,
    #[structopt(short,long,default_value="2",help="show shadowed files/directory (shadowed are e.g. childs of duplicate dirs) (0-3)\n0: show ALL, including pure shadowed groups\n1: show all except pure shadowed groups\n2: show shadowed only if there is also one non-shadowed in the group\n3: never show shadowed\n")]
    pub shadow_rule: u8,
    #[structopt(short,long,default_value="0",help="file lower size limit for scanning in bytes")]
    pub min_size: u64,

    #[structopt(short,long,help="spam stderr")]
    pub verbose: bool,
    #[structopt(long,help="force to display absolute paths")]
    pub absolute: bool,
    #[structopt(long,help="EXPERIMENTAL Enable hashing in 1st pass. Can affect performance positively or negatively")]
    pub pass_1_hash: bool,
    #[structopt(long,help="don't read or write cache file")]
    pub no_cache: bool,
    #[structopt(long,help="abort after pass 1")]
    pub bench_pass_1: bool,
    #[structopt(long,help="EXPERIMENTAL prefetch directory metadata, eventually fails on non-root")]
    pub dir_prefetch: bool,
    #[structopt(short="a",long,help="also search inside archives. requires to scan and hash every archive")]
    pub read_archives: bool, //TODO: build mode w/o archive support
    #[structopt(long,help="EXPERIMENTAL don't scan and reuse cached data")]
    pub no_scan: bool,

    #[structopt(short,long,parse(from_str),default_value="g",help="Results output mode (g/t/d)\ngroups: duplicate entries in sorted size groups\ntree: json as tree\ndiff: like tree, but exact dir comparision, reveals diffs and supersets\n")]
    pub output: OutputMode,
    #[structopt(long,default_value="",help="EXPERIMENTAL")]
    pub dedup: String,

    #[structopt(parse(from_os_str),help="directories to scan. cwd if none given")]
    pub dirs: Vec<PathBuf>,
}

pub enum OutputMode {
    Groups,
    Tree,
    Diff,
}

impl From<&str> for OutputMode {
    fn from(s: &str) -> Self {
        match s.chars().next().map(|c| c.to_ascii_lowercase() ) {
            Some('g') => Self::Groups,
            Some('t') => Self::Tree,
            Some('d') => Self::Diff,
            _ => panic!("Invalid output mode"),
        }
    }
}
