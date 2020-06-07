use dupion::{state::State, opts::Opts, driver::{Driver, platterwalker::PlatterWalker}, phase::Phase, process::{printion, export, calculate_dir_hash, find_shadowed}, util::*, vfs::VfsId, zip::setlocale_hack};
use std::{time::Duration, sync::{atomic::Ordering, RwLock}, path::PathBuf};
use size_format::SizeFormatterBinary;
use structopt::*;

fn main() {
    setlocale_hack();

    let o = OptInput::from_args();

    let opts = Box::leak(Box::new(Opts{
        paths: o.dirs,
        verbose: o.verbose,
        shadow_rule: o.shadow_rule,
        force_absolute_paths: o.absolute_path,
        read_buffer:       ((o.read_buffer * 1048576.0) as usize +1024)/4096*4096,
        prefetch_budget: ((o.prefetch_budget * 1048576.0) as u64 +1024)/4096*4096,
        pass_1_hash: o.pass_1_hash,
        archive_cache_mem: ((o.archive_cache_mem * 1048576.0) as usize +1024)/4096*4096,
        dir_prefetch: o.dir_prefetch,
        read_archives: o.read_archives,
    }));

    if opts.paths.is_empty() {
        opts.paths = vec![std::env::current_dir().unwrap()];
    }

    opts.validate().unwrap();

    let state = Box::leak(Box::new(RwLock::new(State::new(!o.no_cache))));

    if !o.bench_pass_1 {
        state.write().unwrap().eventually_load_vfs();
    }

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

    let mut state = state.write().unwrap();
    state.eventually_store_vfs(true);

    eprintln!("\n\n#### Calculate");
    
    assert!(!state.tree.entries.is_empty(),"No Duplicates found");

    let _ = calculate_dir_hash(&mut state,VfsId::ROOT);
    find_shadowed(&mut state,VfsId::ROOT);

    eprintln!("#### Sort");

    let sorted = export(&state);

    eprintln!("#### Result");

    printion(&sorted, &state, &opts);
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
    let processed_files = disp_processed_files.load(Ordering::Acquire) as u64;
    let relevant_files = disp_relevant_files.load(Ordering::Acquire) as u64;
    let found_files = disp_found_files.load(Ordering::Acquire) as u64;
    let processed_bytes = disp_processed_bytes.load(Ordering::Acquire) as u64;
    let relevant_bytes = disp_relevant_bytes.load(Ordering::Acquire) as u64;
    let found_bytes = disp_found_bytes.load(Ordering::Acquire) as u64;
    let prev_bytes = disp_prev.swap(processed_bytes as usize, Ordering::AcqRel) as u64;
    let alloced = alloc_mon.load(Ordering::Acquire) as u64;
    assert!(processed_bytes >= prev_bytes);

    eprint!(
        //"\x1B[2K\rAnalyzed files: {:>filefill$}/{} bytes: {:>12}B/{}B ({:>12}B/s) percent: {}%",
        "\x1B[2K\rFound: {:>12} ({:>12}B)                Hashed: {:>12}/{} {:>12}B/{}B ({:>12}B/s)        alloc={:>12}B",
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
}

#[derive(StructOpt)]
#[structopt(name = "dedupion", about = "Find duplicate files and folders")]
pub struct OptInput {
    #[structopt(long,default_value="1.0",help="read buffer in MiB, has no effect")]
    pub read_buffer: f64,
    #[structopt(long,default_value="16.0",help="prefetch budget in MiB")]
    pub prefetch_budget: f64,
    #[structopt(long,default_value="1024.0",help="TODO in MiB")]
    pub archive_cache_mem: f64,
    #[structopt(short,long,default_value="2",help="show shadowed files/directory (shadowed are e.g. childs of duplicate dirs) (0-3)")]
    pub shadow_rule: u8,

    #[structopt(short,long,help="spam stderr")]
    pub verbose: bool,
    #[structopt(short,long,help="force to display paths absolute")]
    pub absolute_path: bool,
    #[structopt(long,help="Enable hashing in 1st pass. Can affect performance positively or negatively")]
    pub pass_1_hash: bool,
    #[structopt(long,help="don't read or write cache file")]
    pub no_cache: bool,
    #[structopt(long,help="abort after pass 1")]
    pub bench_pass_1: bool,
    #[structopt(short="p",long,help="prefetch directory metadata. recommended to use, but eventually fails on non-root")]
    pub dir_prefetch: bool,
    #[structopt(short="a",long,help="also search inside archives. requires to scan and hash every archive")]
    pub read_archives: bool,

    #[structopt(parse(from_os_str),help="directories to scan. cwd if none given")]
    pub dirs: Vec<PathBuf>,
}