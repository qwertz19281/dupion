use super::*;
use std::{io::ErrorKind, path::Path};
use std::os::unix::fs::MetadataExt;
use platter_walk::{Order, ToScan};
use vfs::VfsId;
use reapfrog::MultiFileReadahead;
use std::{sync::{atomic::Ordering, Arc}, io::{Read, Write, self}, fs::{Metadata, File}};
use util::*;
use zip::{open_zip, decode_zip};
use io::{BufReader, Cursor};

pub struct PlatterWalker {
    pub entries: Option<Vec<(u64,VfsId)>>,
}

impl Driver for PlatterWalker {
    fn run(&mut self, state: &'static RwLock<State>, opts: &'static Opts, phase: Phase) -> AnyhowResult<()> {
        match phase {
            Phase::Size => {
                assert!(self.entries.is_none());

                let mut scan = ToScan::<()>::new();

                scan.prefetch_dirs(opts.dir_prefetch);
                scan.set_order(Order::Content);
                //scan.set_batchsize(usize::MAX);

                let mut dest = Vec::with_capacity(65536);
                let mut hash_now = Vec::new();

                for root in &opts.paths {
                    if root.is_dir() {
                        try_returnerr!(scan.add_root(root.clone()),"\tError adding root: {} ({})",opts.path_disp(root));
                    } else if root.is_file() {
                        size_file(
                            root,
                            &root.metadata()?,
                            0,
                            &mut dest,
                            &mut hash_now,
                            &mut state.write(), opts
                        )?;
                    }
                }

                scan.set_prefilter(Box::new(move |path,ft,_| {
                    ft.is_file() && !ft.is_symlink() && path.to_str().is_some()
                }));

                for entry_set in scan {
                    let mut s = state.write();

                    if let Some(entry_set) = soft_error!(entry_set,"\tError: {}",) {
                        for (phy_off, mut entry) in entry_set {

                            let path = match entry.canon_path.take() {
                                Some(c) => c,
                                None => {
                                    dprintln!("METAMISS");
                                    try_continue!(entry.path().canonicalize(),"\tError: {} ({})",opts.path_disp(entry.path()))
                                },
                            };
                            let meta = match entry.metadata.take() {
                                Some(c) => c,
                                None => {
                                    dprintln!("METAMISS");
                                    try_continue!(path.metadata(),"\tError: {} ({})",opts.path_disp(&path))
                                },
                            };

                            size_file(&path, &meta, phy_off, &mut dest, &mut hash_now, &mut s, opts)?;
                        }

                        drop(s);

                        //dprintln!("\tPreHash {}",hash_now.len());

                        if opts.pass_1_hash {
                            hash_files(
                                hash_now.iter().cloned(),
                                state,
                                opts,
                                true,
                            )?;
                        }

                        hash_now.clear();
                    }
                }

                let mut s = state.write();

                DISP_ENABLED.store(false, Ordering::Relaxed);

                dprint!("\nSort...");
                io::stdout().flush().unwrap();

                dest.sort_by_key(|(o,_)| *o );

                for (_,id) in &dest {
                    if s.is_file_read_candidate(*id,opts) {
                        s.tree[*id].disp_add_relevant();
                    }
                }

                dest.shrink_to_fit();

                dprintln!("Sort... Done");

                DISP_ENABLED.store(true, Ordering::Relaxed);

                self.entries = Some(dest);

                Ok(())
            },
            Phase::Hash => {
                assert!(self.entries.is_some());

                hash_files(
                    self.entries.as_ref().unwrap().iter().map(|(_,id)| *id ),
                    state,
                    opts,
                    true,
                )?;

                
                Ok(())
            },
            Phase::PostHash => {
                assert!(self.entries.is_some());

                hash_files(
                    self.entries.as_ref().unwrap().iter().map(|(_,id)| *id ),
                    state,
                    opts,
                    false,
                )?;
                
                Ok(())
            }
        }
    }
    fn new() -> Self {
        Self{
            entries: None,
        }
    }
}

pub fn size_file(path: &Path, meta: &Metadata, phy_off: u64, dest: &mut Vec<(u64,VfsId)>, hash_now: &mut Vec<VfsId>, s: &mut State, opts: &Opts) -> AnyhowResult<()> {
    let size = meta.len();
    let ctime = meta.ctime();

    opts.log_verbosed("SIZE", path);

    DISP_FOUND_BYTES.fetch_add(size,Ordering::Relaxed);
    DISP_FOUND_FILES.fetch_add(1,Ordering::Relaxed);

    let id = s.tree.cid_and_create(path);
    s.validate(id,ctime,Some(size),None);

    let e = &mut s.tree[id];
    e.is_file = true;
    
    e.file_size = Some(size);
    e.phys = Some(phy_off);
    
    s.push_to_size_group(id,true,false).unwrap();
    if s.tree[id].file_hash.is_some() {
        s.push_to_hash_group(id,true,false).unwrap();
        //disp_processed_bytes.fetch_add(size as usize,Ordering::Relaxed);
        //disp_processed_files.fetch_add(1,Ordering::Relaxed);
    }

    if s.is_file_read_candidate(id,opts) {
        s.tree[id].disp_add_relevant();
    }

    dest.push((phy_off,id));
    hash_now.push(id);
    Ok(())
}

pub fn hash_files(i: impl Iterator<Item=VfsId>+Send, s: &'static RwLock<State>, opts: &'static Opts, do_zips: bool) -> AnyhowResult<()> {
    #[derive(Clone)]
    struct Reapion {
        path: Arc<Path>,
        id: VfsId,
    }
    impl AsRef<Path> for Reapion {
        fn as_ref(&self) -> &Path {
            &self.path
        }
    }

    let read_mutex = ZeroLock::new(true);
    let read_mutex = &read_mutex;

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(opts.threads)
        .build()
        .unwrap();

    pool.scope(move |pool| {
        let filtered = i.filter_map(|id| {
            let mut s = s.write();
            let do_hash = s.is_file_read_candidate(id,opts);
            let e = &mut s.tree[id];
            if do_hash {
                e.disp_add_relevant();
                assert!(e.valid);
                let path = e.path.clone();
                Some(Reapion{
                    path,
                    id,
                })
            }else{
                None
            }
        });

        let mut reaper = MultiFileReadahead::new(filtered);

        reaper.dropbehind(opts.cache_dropbehind);

        let huge_zip_thres = opts.archive_cache_mem as u64 / opts.threads as u64;

        let read_buffer = opts.read_buffer;

        let mut buf = vec![0;read_buffer];

        let mut local_read_lock = read_mutex.clone();

        let mut cache_watcher = CacheUsable::new(1024*1024 .. opts.prefetch_budget);

        local_read_lock.lock();
        //local_read_lock = None;

        'big: loop {
            let budget = cache_watcher.get();
            //dprintln!("Budget: {}",budget);
            reaper.budget(budget);
            match reaper.next() {
                None => break,
                Some(Err(e)) => {
                    dprintln!("\tError {}",e);
                }
                Some(Ok(mut reader)) => {
                    let Reapion{path: p,id} = reader.data().clone(); 

                    let size = reader.metadata().size();
                    let ctime = reader.metadata().ctime();

                    {
                        let s = s.read();
                        if s.tree[id].file_size != Some(size) || s.tree[id].ctime != Some(ctime) {
                            dprintln!("\tSkip comodified file: {}",opts.path_disp(&p));
                            continue;
                        }
                    };

                    opts.log_verbosed("HASH", &p);

                    let mut hasher = blake3::Hasher::new();

                    let mut reader = &mut reader;

                    if do_zips && opts.zip_by_extension(&p) && size <= huge_zip_thres {
                        let mut buf = AllocMonBuf::new(size as usize, opts.archive_cache_mem);

                        let mut off = 0;

                        loop {
                            match reader.read(&mut buf[off..(off+read_buffer).min(size as usize)]) {
                                Ok(0) => break,
                                Ok(n) => {
                                    hasher.update(&buf[off..off+n]);
                                    off+=n;
                                    DISP_PROCESSED_BYTES.fetch_add(n as u64,Ordering::Relaxed);
                                },
                                Err(e) if e.kind() == ErrorKind::UnexpectedEof => break,
                                Err(e) if e.kind() == ErrorKind::Interrupted => {}
                                Err(e) => {
                                    dprintln!("\tFailed to read {}",e);
                                    continue 'big;
                                },
                            }
                        }
                        local_read_lock.unlock();
                        assert_eq!(off,size as usize);

                        let p = p.clone();
                        pool.spawn(move |_| {
                            let buf: AllocMonBuf = buf;
                            let r = try_return!(open_zip(Cursor::new(&buf[..size as usize]),&p,s,opts),"\tFailed to open ZIP: {} ({})",opts.path_disp(&p));
                            try_return!(decode_zip(r,&p,s,opts),"\tFailed to read ZIP: {} ({})",opts.path_disp(&p));
                        });
                    }else{
                        if do_zips && opts.zip_by_extension(&p) {
                            DISP_RELEVANT_BYTES.fetch_add(size,Ordering::Relaxed);
                            DISP_RELEVANT_FILES.fetch_add(1,Ordering::Relaxed);
                            let p = p.clone();
                            pool.spawn(move |_| {
                                let (size,path) = {
                                    let s = s.read();
                                    s.eventually_store_vfs(&opts.cache_path, false);
                                    let e = &s.tree[id];
                                    ( e.file_size.unwrap(), e.path.clone() )
                                };
                                let reader = try_return!(File::open(&path),"\tFailed to open file for big zip read: {} ({})",opts.path_disp(&p));
                                let reader = MutexedReader{inner: reader,mutex: read_mutex.clone()};
                                let reader = BufReader::with_capacity(64*1024*1024,reader);
                        
                                let r = try_return!(open_zip(reader,&path,s,opts),"\tFailed to open ZIP: {} ({})",opts.path_disp(&p));
                                try_return!(decode_zip(r,&path,s,opts),"\tFailed to read ZIP: {} ({})",opts.path_disp(&p));
                        
                                DISP_PROCESSED_BYTES.fetch_add(size,Ordering::Relaxed);
                                DISP_PROCESSED_FILES.fetch_add(1,Ordering::Relaxed);
                            });
                        }
                        loop {
                            match reader.read(&mut buf[0..read_buffer]) {
                                Ok(0) => break,
                                Ok(n) => {
                                    hasher.update(&buf[..n]);
                                    DISP_PROCESSED_BYTES.fetch_add(n as u64,Ordering::Relaxed);
                                },
                                Err(e) if e.kind() == ErrorKind::UnexpectedEof => break,
                                Err(e) if e.kind() == ErrorKind::Interrupted => {}
                                Err(e) => {
                                    dprintln!("\tFailed to read {}",e);
                                    continue 'big;
                                },
                            }
                        }
                        local_read_lock.unlock();
                    }
                    local_read_lock.unlock();

                    let hash = Arc::new(hasher.finalize().into());

                    let mut s = s.write();

                    let entry = &mut s.tree[id];

                    entry.file_hash = Some(hash);

                    if opts.zip_by_extension(&p) {
                        entry.is_dir = true;
                    }

                    //let size = entry.file_size.unwrap() as usize;

                    s.push_to_hash_group(id,true,false).unwrap();

                    //state.disp_pass_2_processed_bytes_capped += size.max(1024*1024);
                    DISP_PROCESSED_FILES.fetch_add(1,Ordering::Relaxed);

                    s.eventually_store_vfs(&opts.cache_path, false);

                    drop(s);

                    local_read_lock.lock();
                }
            }
        }
        local_read_lock.unlock();
        Ok(())
    })
}

#[macro_export]
macro_rules! try_continue {
    ($oof:expr,$fmt:expr,$($args:tt)*) => {
        match $oof {
            Ok(f) => {
                f
            },
            Err(e) => {
                dprintln!($fmt,e,$($args)*);
                continue
            },
        }
    };
}

#[macro_export]
macro_rules! try_return {
    ($oof:expr,$fmt:expr,$($args:tt)*) => {
        match $oof {
            Ok(f) => {
                f
            },
            Err(e) => {
                dprintln!($fmt,e,$($args)*);
                return
            },
        }
    };
}

#[macro_export]
macro_rules! soft_error {
    ($oof:expr,$fmt:expr,$($args:tt)*) => {
        match $oof {
            Ok(f) => {
                Some(f)
            },
            Err(e) => {
                dprintln!($fmt,e,$($args)*);
                None
            },
        }
    };
}

#[macro_export]
macro_rules! try_returnerr {
    ($oof:expr,$fmt:expr,$($args:tt)*) => {
        match $oof {
            Ok(f) => {
                f
            },
            Err(e) => {
                dprintln!($fmt,e,$($args)*);
                return Err(e.into())
            },
        }
    };
}
