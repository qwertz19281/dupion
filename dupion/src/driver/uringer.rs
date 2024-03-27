use std::borrow::Borrow;
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::Ordering::Relaxed;
use std::time::{Duration, SystemTime};

use blake3::Hasher;
use glommio::io::{BufferedFile, ReadResult};
use glommio::sys::StatxTimestamp;
use glommio::{executor, Latency, LocalExecutor, LocalExecutorBuilder, Placement, Shares, TaskQueueHandle};
use parking_lot::RwLock;
use size_format::SizeFormatterBinary;
use walkdir::WalkDir;

use crate::opts::Opts;
use crate::phase::Phase;
use crate::soft_error;
use crate::state::State;
use crate::util::{Hash, DISP_PROCESSED_BYTES, DISP_PROCESSED_FILES, DISP_RELEVANT_BYTES, DISP_RELEVANT_FILES};
use crate::vfs::VfsId;

use super::fiemap::{read_fiemap, FiemapInfo, ReadFiemapError};
use super::{common, Driver};

pub struct Uringer {
    pub entries: &'static RefCell<Option<Vec<VfsId>>>,
    pub ex: LocalExecutor,
}

impl Driver for Uringer {
    fn run(&mut self, state: &'static RwLock<State>, opts: &'static Opts, phase: Phase) -> anyhow::Result<()> {
        match phase {
            Phase::Size => {
                assert!(self.entries.borrow().is_none());

                find_files(
                    self,
                    state,
                    opts,
                )?;

                Ok(())
            },
            Phase::Hash => {
                assert!(self.entries.borrow().is_some());

                hash_files(
                    self,
                    state,
                    opts,
                )?;
                
                Ok(())
            },
            Phase::PostHash => {
                assert!(self.entries.borrow().is_some());
                
                Ok(())
            }
        }
    }
    fn new(opts: &'static Opts) -> Self {
        Self{
            entries: Box::leak(Box::new(RefCell::new(None))),
            ex: LocalExecutorBuilder::new(Placement::Unbound)
                .blocking_thread_pool_placement(glommio::PoolPlacement::Unbound(8))
                .spin_before_park(Duration::from_micros(16))
                .io_memory(opts.prefetch_budget as usize)
                .ring_depth(2048)
                .make().unwrap(),
        }
    }
}

impl Drop for Uringer {
    fn drop(&mut self) {
        if let Ok(mut v) = self.entries.try_borrow_mut() {
            *v = None;
        }
    }
}

fn find_files(ringer: &Uringer, state: &'static RwLock<State>, opts: &'static Opts) -> anyhow::Result<()> {
    let entries = ringer.entries;

    *entries.borrow_mut() = Some(Vec::with_capacity(65536));

    let walkdir_max_files = 16;
    let max_files = opts.limit_open_files(walkdir_max_files, 4, 64);

    let mut state = state.write();
    let state = &mut *state;
    let mut entries = entries.borrow_mut();
    let entries = entries.as_mut().unwrap();

    std::thread::scope(|s| {
        let (send,recv) = crossbeam_channel::bounded::<(usize,PathBuf,u64,StatxTimestamp,u32)>(max_files*2);
        let send = &send;

        s.spawn(move || {
            while let Ok((idx,path,size,ctime,uid)) = recv.recv() {
                let id = common::size_file(
                    &path,
                    size, ctime.tv_sec, uid,
                    None, None,
                    state, opts
                );
                if idx >= entries.len() {
                    entries.resize(idx+1, VfsId::ROOT);
                }
                entries[idx] = id;
            }
        });

        let ins_idx = &Cell::new(0);

        ringer.ex.run(async {
            let tq = glommio::executor()
                .create_task_queue(Shares::Static(16), Latency::NotImportant, "dupion_findfiles");

            let mut bufbuf = VecDeque::new();

            for root in &opts.paths {
                let Some(root) = soft_error!(root.canonicalize(),"\tError: {}",) else {continue};

                for f in WalkDir::new(root).follow_links(false).max_open(walkdir_max_files) {
                    let Some(f) = soft_error!(f,"\tError: {}",) else {continue};

                    if !f.file_type().is_file() {continue;}
                    let path = f.path().to_owned();

                    let t = async move {
                        let path = path;
                        let statx = glommio::io::statx(&path).await;
                        if let Some(statx) = soft_error!(statx,"\tError: {}",) {
                            let idx = ins_idx.get();
                            ins_idx.set(idx+1);
                            Some((idx,path,statx.stx_size,statx.stx_ctime,statx.stx_uid))
                        } else {
                            None
                        }
                    };

                    bufbuf.push_back(unsafe{glommio::spawn_scoped_local_into(t,tq)}.unwrap());

                    if bufbuf.len() >= max_files {
                        for b in bufbuf.drain(..) {
                            if let Some(v) = b.await {
                                send.send(v).unwrap();
                            }
                        }
                    }
                }
            }
            for b in bufbuf.drain(..) {
                if let Some(v) = b.await {
                    send.send(v).unwrap();
                }
            }
        });
    });

    Ok(())
}

fn hash_files(ringer: &Uringer, state: &'static RwLock<State>, opts: &'static Opts) -> anyhow::Result<()> {
    let entries = ringer.entries;

    let mut entries = entries.borrow_mut();
    let entries = entries.as_mut().unwrap();

    let mut state = state.write();
    let state = RefCell::new(&mut *state);

    let mut filtered = entries.into_iter().filter_map(|id| {
        let mut s = state.borrow_mut();
        let do_hash = s.is_file_read_candidate(*id,opts);
        let e = &mut s.tree[*id];
        if do_hash {
            e.disp_add_relevant();
            assert!(e.valid);
            let path = e.path.clone();
            Some(Rc::new(BatchFile::new(path,*id)))
        }else{
            None
        }
    });

    let dafiles = 216.min(opts.max_open_files as usize);

    let ordion = &Cell::new(0);

    ringer.ex.run(async {
        let mut b = Batches {
            smallbatch: Default::default(),
            bigbatch: Default::default(),
            prebatch: Default::default(),
            batch_size_max: dafiles,
            batch_space_max: opts.prefetch_budget,
            bigfile_thresh: (dafiles as u64/4).saturating_sub(MIBIBYTE).max(8*MIBIBYTE),
            bigbatch_limit: 4,
        };

        let big_file_read_size = 8*MIBIBYTE.min(b.bigfile_thresh);

        let tq1 = glommio::executor()
            .create_task_queue(Shares::Static(16), Latency::NotImportant, "dupion_hashfiles");

        'superloop: loop {
            if !upump_batch(&mut b, &mut filtered, &state).await {break 'superloop;}

            // dprintln!(
            //     "BATCH SMOL {} BEEG {} PRE {} ({}B)",
            //     b.smallbatch.len(), b.bigbatch.len(),
            //     b.prebatch.len(),
            //     SizeFormatterBinary::new(b.batches_total_size(&*state.borrow())),
            // );

            uopen_files(b.smallbatch.iter().cloned(), tq1, ordion, &state, opts).await;

            {
                let spawned = b.smallbatch.iter().map(|f|
                    uspawn_read_small_file(f.clone(), tq1, &state, opts)
                ).collect::<Vec<_>>();

                for (s,fi) in spawned.into_iter().zip(b.smallbatch.iter()) {
                    if let Some(s) = s {
                        let s = s.await;
                        // if s && fi.overread.borrow().is_some() {
                        //     b.bigbatch.push_back(fi.clone());
                        // }
                        // try_resultize(&mut b.result_ordered, false, &mut resper);
                    }
                }
            }

            uclose_files(b.smallbatch.drain(..), tq1, |fi| true /*fi.overread.borrow().is_none()*/ ).await;

            for fi in b.bigbatch.iter().cloned() {
                if let Some(v) = uspawn_open_single_file(fi.clone(), tq1, &ordion, &state, opts) {v.await;}

                uread_big_file(fi, big_file_read_size as usize, &state, opts).await;
            }

            uclose_files(b.bigbatch.drain(..), tq1, |_| true).await;

            // try_resultize(&mut b.result_ordered, true, &mut resper);

            // b.result_ordered.clear();

            state.borrow_mut().eventually_store_vfs(&opts.cache_path, false);
        }
    });

    // DISP_RELEVANT_FILES.store(0, Relaxed);
    // DISP_RELEVANT_BYTES.store(0, Relaxed);

    let mut state = state.borrow_mut();

    for id in &**entries {
        if state.is_file_read_candidate(*id,opts) {
            //state.tree[*id].disp_relevated = false;
            state.tree[*id].disp_add_relevant();
        }
    }

    Ok(())
}

pub type RcBatchFile = Rc<BatchFile>;

pub struct BatchFile {
    pub path: Arc<Path>,
    pub id: VfsId,
    pub fiemap: RefCell<Option<FiemapInfo>>,
    pub ordion: Cell<u64>,
    pub open: RefCell<Option<BufferedFile>>,
    pub error: RefCell<Option<Box<dyn std::error::Error>>>,
    // As soon we read more than expected in a smallfile, we would move it over to bigbatch can continue in big mode
    // pub overread: RefCell<Option<(u64,Hasher)>>,
}

impl BatchFile {
    fn new(path: Arc<Path>, id: VfsId) -> Self {
        Self {
            path,
            id,
            fiemap: RefCell::new(None),
            ordion: Cell::new(0),
            open: RefCell::new(None),
            error: RefCell::new(None),
            // overread: RefCell::new(None),
        }
    }

    fn is_open(&self) -> bool {
        let borrow = self.open.try_borrow();
        debug_assert!(borrow.is_ok());
        borrow.map_or(true, |v| v.is_some())
    }
}

const SMALL_FILE_OVERREAD: u64 = 4096;
const SMALL_FILE_ATTACHBUF: u64 = 16384;

fn uspawn_open_single_file<'a>(f: RcBatchFile, tq: TaskQueueHandle, ordion: &'a Cell<u64>, state: &'a RefCell<&mut State>, opts: &'static Opts) -> Option<glommio::ScopedTask<'a,bool>> {
    if f.open.borrow().is_none() && f.error.borrow().is_none() {
        let fut = async move {
            let mut open_flags = 0;
            if Some(opts.euid) == state.borrow().tree[f.id].uid {
                open_flags |= libc::O_NOATIME;
            }
            
            match glommio::io::OpenOptions::new().read(true).custom_flags(open_flags).buffered_open(&*f.path).await {
                Ok(v) => {
                    let ord = ordion.get();
                    f.ordion.set(ord);
                    ordion.set(ord+1);
                    
                    let new_size = v.init_stats().unwrap().stx_size;
                    let new_ctime = v.init_stats().unwrap().stx_ctime.tv_sec;
                    let mut state = state.borrow_mut();
                    let s = &mut **state;
                    let entry = &mut s.tree[f.id];

                    if entry.file_size != Some(new_size) || entry.ctime != Some(new_ctime) {
                        dprintln!("\tSkip comodified file: {}",opts.path_disp(&f.path));
                        DISP_RELEVANT_FILES.fetch_sub(1, Relaxed);
                        DISP_RELEVANT_BYTES.fetch_sub(entry.file_size.unwrap_or(0), Relaxed);
                        drop(state);
                        utryclose_single_file(v).await;
                        return false;
                    }

                    entry.file_size = Some(new_size);
                    entry.ctime = Some(new_ctime);
                    //dbg!(v.init_stats().unwrap());
                    //if f.size.get().unwrap() < bigfile_thresh {f.phys.set(read_file_fiemap(&v));}

                    let mut cancel_read = false;
                    
                    if opts.fiemap != 0 {
                        let fiemap = read_fiemap(&v, true, true, true, opts.fiemap);

                        match fiemap {
                            Ok(Some(fm)) => {
                                *f.fiemap.borrow_mut() = Some(fm.clone());
                                entry.phys = Some(fm.phys);
                                entry.n_extents = Some(fm.n_extents);
                                if let Some(h) = fm.fiemap_hash.clone() {
                                    if let Some(h) = s.fiemap2hash.get(&(new_size,h)).cloned() {
                                        //dprintln!("FIEMAP SKIP EVENT {:?}",&fm);
                                        entry.file_hash = Some(h);
                                        cancel_read = true;
                                        // DISP_PROCESSED_BYTES.fetch_add(new_size, Relaxed);
                                        // DISP_PROCESSED_FILES.fetch_add(1, Relaxed);
                                    }
                                }
                            },
                            Ok(None) | Err(ReadFiemapError::ExtentLimitExceeded) =>
                                if opts.skip_no_phys {cancel_read = true;},
                            Err(e) => dprintln!("\tError reading FIEMAP of {}: {}",opts.path_disp(&f.path),e),
                        }
                    }

                    if cancel_read {
                        DISP_RELEVANT_FILES.fetch_sub(1, Relaxed);
                        DISP_RELEVANT_BYTES.fetch_sub(entry.file_size.unwrap_or(0), Relaxed);
                        drop(state);
                        utryclose_single_file(v).await;
                        return false;
                    }

                    *f.open.borrow_mut() = Some(v);
                    true
                },
                Err(e) => {
                    dprintln!("Error opening file {}: {e}", f.path.to_string_lossy());
                    *f.error.borrow_mut() = Some(Box::new(e));
                    false
                }
            }
        };
        Some(unsafe{glommio::spawn_scoped_local_into(fut, tq)}.unwrap())
    } else {
        None
    }
}

async fn uopen_files(f: impl Iterator<Item=RcBatchFile>, tq: TaskQueueHandle, ordion: &Cell<u64>, state: &RefCell<&mut State>, opts: &'static Opts) {
    let spawned = f.map(|f| uspawn_open_single_file(f, tq, ordion, state, opts) ).collect::<Vec<_>>();

    for s in spawned {
        if let Some(s) = s {
            s.await;
        }
    }
}

fn uspawn_close_single_file(f: RcBatchFile, tq: TaskQueueHandle, mut pred: impl FnMut(&RcBatchFile) -> bool) -> Option<glommio::Task<glommio::Result<(),()>>> {
    if !pred(&f) {
        return None;
    }
    if let Some(v) = f.open.try_borrow_mut().ok().and_then(|mut f| f.take()) {
        Some(glommio::spawn_local_into(async move {
            v.close().await
        }, tq).unwrap())
    } else {
        None
    }
}

async fn utryclose_single_file(f: BufferedFile) {
    if let Err(e) = f.close().await {
        dprintln!("Error closing file: {e}");
    }
}

async fn uclose_files(f: impl Iterator<Item=RcBatchFile>, tq: TaskQueueHandle, mut pred: impl FnMut(&RcBatchFile) -> bool) {
    let spawned = f.map(|f| uspawn_close_single_file(f, tq, &mut pred) ).collect::<Vec<_>>();

    for s in spawned {
        if let Some(s) = s {
            if let Err(e) = s.await {
                dprintln!("Error closing file: {e}");
            }
        }
    }
}

fn file_read_success(f: RcBatchFile, read: u64, fsize: u64, hash: Hash, state: &mut State, opts: &'static Opts) -> bool {
    //CURRENT_FILE.swap(Some(f.file.path.clone()), AcqRel);
    DISP_PROCESSED_FILES.fetch_add(1, Relaxed);

    if read != fsize {
        dprintln!("\tSkip comodified file2: {}",opts.path_disp(&f.path));
        return false;
    }

    state.tree[f.id].file_hash = Some(hash.clone());

    state.push_to_hash_group(f.id,true,false).unwrap();
    if opts.fiemap > 1 {
        if let Some(v) = f.fiemap.borrow().as_ref().and_then(|v| v.fiemap_hash.as_ref() ) {
            state.fiemap2hash.insert((fsize,v.clone()), hash);
        }
    }

    return true;
}

fn uspawn_read_small_file<'a>(f: RcBatchFile, tq: TaskQueueHandle, state: &'a RefCell<&mut State>, opts: &'static Opts) -> Option<glommio::ScopedTask<'a,bool>> {
    if f.is_open() && f.error.borrow().is_none() && state.borrow().tree[f.id].file_hash.is_none() {
        let fsize = state.borrow().tree[f.id].file_size.unwrap();
        let fut = async move {
            let mut hasher = Some(Hasher::new());
            let mut read = 0;
            let mut prev_readres = {
                let open_file = f.open.borrow();
                let open_file = open_file.as_ref().unwrap();
                fadvise_sequential(open_file);
                Some(open_file.read_at(read, (fsize + SMALL_FILE_OVERREAD) as usize).await)
            };
            loop {
                let res = match prev_readres.take() {
                    Some(Ok(v)) => v,
                    Some(Err(e)) => {
                        dprintln!("Error reading file {}: {e}", f.path.to_string_lossy());
                        *f.error.borrow_mut() = Some(Box::new(e));
                        return false;
                    },
                    None => panic!(),
                };

                DISP_PROCESSED_BYTES.fetch_add(res.len() as u64, Relaxed);
                read += res.len() as u64;

                if res.is_empty() {
                    return file_read_success(
                        f,
                        read, fsize,
                        Arc::new(hasher.unwrap().finalize().into()),
                        &mut *state.borrow_mut(), opts
                    );
                }

                let mut h = hasher.take().unwrap();

                // if read > fsize {
                //     let (_,h) = ublocking_with_readbuf(res.clone(), move |buf| {
                //         h.update(buf);
                //         h
                //     }).await;
                //     *f.overread.borrow_mut() = Some((read,h));
                //     return true;
                // }

                let hash_task = ublocking_with_readbuf(res.clone(), move |buf| {
                    h.update(buf);
                    h
                });
                let (next,(_,h)) = glommio::futures_lite::future::zip(
                    f.open.borrow().as_ref().unwrap().read_at(read, SMALL_FILE_OVERREAD as usize),
                    hash_task
                ).await;

                prev_readres = Some(next);
                hasher = Some(h);
            }
        };
        Some(unsafe{glommio::spawn_scoped_local_into(fut, tq)}.unwrap())
    } else {
        None
    }
}

async fn uread_big_file(f: RcBatchFile, big_file_read_size: usize, state: &RefCell<&mut State>, opts: &'static Opts) -> Option<bool> {
    if f.is_open() && f.error.borrow().is_none() && state.borrow().tree[f.id].file_hash.is_none() {
        //CURRENT_FILE.swap(Some(f.file.path.clone()), AcqRel);
        let fsize = state.borrow().tree[f.id].file_size.unwrap();
        let (mut read, mut hasher) = {
            // let mut v = f.overread.borrow_mut();
            // if let Some((read, hasher)) = v.take() {
            //     (read, Some(hasher))
            // } else {
                (0, Some(Hasher::new()))
            // }
        };
        let mut prev_readres = {
            let open_file = f.open.borrow();
            let open_file = open_file.as_ref().unwrap();
            fadvise_sequential(open_file);
            Some(open_file.read_at(read, big_file_read_size).await)
        };
        loop {
            let res = match prev_readres.take() {
                Some(Ok(v)) => v,
                Some(Err(e)) => {
                    dprintln!("Error reading file {}: {e}", f.path.to_string_lossy());
                    *f.error.borrow_mut() = Some(Box::new(e));
                    return Some(false);
                },
                None => panic!(),
            };
            DISP_PROCESSED_BYTES.fetch_add(res.len() as u64, Relaxed);
            read += res.len() as u64;
            if res.is_empty() {
                return Some(file_read_success(
                    f,
                    read, fsize,
                    Arc::new(hasher.unwrap().finalize().into()),
                    &mut *state.borrow_mut(), opts
                ));
            }

            let mut h = hasher.take().unwrap();

            let hash_task = ublocking_with_readbuf(res.clone(), move |buf| {
                h.update(buf);
                h
            });
            let (next,(_,h)) = glommio::futures_lite::future::zip(
                f.open.borrow().as_ref().unwrap().read_at(read, big_file_read_size),
                hash_task
            ).await;

            prev_readres = Some(next);
            hasher = Some(h);
        }
    } else {
        None
    }
}

const MIBIBYTE: u64 = 1024*1024;

struct Batches {
    // These two bufs should be empty/cleared after eatch batchop
    smallbatch: VecDeque<RcBatchFile>,
    bigbatch: VecDeque<RcBatchFile>, // TODO bigbatch should be sorted by (srcid,...), right before running bigbatches we should use as bufsize (batch_space_max/n_sources_parallel/2)
    // These should not be cleared after the previous batch
    prebatch: VecDeque<RcBatchFile>,
    // This one holds all and has to be in natural order
    // Nontheless this one should be fully cleared after a batchop
    // After every await we could soft-try to advance in here, which means to check if the next is done or error, we can pop it and print it, for faster/smoother output
    //result_ordered: VecDeque<RcBatchFile>,
    // How many files we can open at one time (maxopenfiles-walkdiropenfiles).min((nr_requests/2)*10/9).max(1)
    batch_size_max: usize,
    // How much space the small files in batch can use
    batch_space_max: u64,
    // should be smaller than (batch_space_max/n_sources_parallel.max(2)) - attachbuf_big - 4096
    bigfile_thresh: u64,
    bigbatch_limit: usize,
}

impl Batches {
    // returns batch file and open awaiter
    fn pump(&mut self, mut pump_src: impl Iterator<Item=RcBatchFile>) -> Option<RcBatchFile> {
        if let Some(batch) = self.prebatch.pop_front() {
            Some(batch)
        } else {
            pump_src.next()
        }
    }

    fn squish_back_into_pump(&mut self, fi: RcBatchFile) {
        self.prebatch.push_front(fi);
    }

    fn batches_total_size(&self, state: &State) -> u64 {
        self.smallbatch.iter().chain(self.bigbatch.iter()).map(|f| state.tree[f.id].file_size.unwrap() ).sum()
    }
}

async fn upump_batch(b: &mut Batches, mut pump_src: impl Iterator<Item=RcBatchFile>, state: &RefCell<&mut State>) -> bool {
    assert!(b.smallbatch.is_empty() && b.bigbatch.is_empty() /*&& batches.smallbatch_open == 0 && batches.bigbatch_open == 0*/);
    assert!(b.bigfile_thresh*2 <= b.batch_space_max);
    //assert!(b.prebatch_open.len() <= b.microopen_check_size);

    let mut small_batch_space: u64 = b.smallbatch.iter().map(|f| state.borrow().tree[f.id].file_size.unwrap() ).sum();

    let mut pumped_anything = false;

    loop {
        let mut active_open_files = 0;
        for fi in &b.bigbatch {
            if fi.is_open() {
                active_open_files += 1;
            }
        }

        let Some(fi) = b.pump(&mut pump_src) else {break};
        pumped_anything = true;

        // if fi.error.borrow().is_some() {
        //     b.result_ordered.push_back(fi.clone());
        //     continue;
        // }

        let fsize = state.borrow().tree[fi.id].file_size.unwrap();

        if fsize >= b.bigfile_thresh {
            if b.bigbatch.len() < b.bigbatch_limit {
                // b.result_ordered.push_back(fi.clone());
                b.bigbatch.push_back(fi);
            } else {
                b.squish_back_into_pump(fi);
                break;
            }
        } else {
            let rsize = fsize + SMALL_FILE_OVERREAD + SMALL_FILE_ATTACHBUF;
            if active_open_files + b.smallbatch.len() < b.batch_size_max && small_batch_space + rsize <= b.batch_space_max {
                // b.result_ordered.push_back(fi.clone());
                b.smallbatch.push_back(fi);
                small_batch_space += rsize;
            } else {
                b.squish_back_into_pump(fi);
                break;
            }
        }
    }

    // b.result_ordered.make_contiguous().sort_by_key(|fi| fi.file.orderid );

    pumped_anything
}

// fn try_resultize(result_ordered: &mut VecDeque<RcBatchFile>, force: bool, mut dest: impl FnMut(RcBatchFile)) {
//     loop {
//         let Some(v) = result_ordered.front() else {return};
//         if !force && v.hash.borrow().is_none() {return;}
//         let v = result_ordered.pop_front().unwrap();
//         dest(v);
//     }
// }

async fn ublocking_with_readbuf<F,R>(buf: ReadResult, func: F) -> (ReadResult,R)
where
    F: for<'a> FnOnce(&'a [u8]) -> R + Send + 'static,
    R: Send + 'static,
{
    struct BullRef(*const [u8]);
    // SAFETY: The raw ptr inside will be valid and dereferencable for the entire duration the FnOnce can run. The sole owner, the Box, is valid for the entire duration
    unsafe impl Send for BullRef {}

    #[inline(always)]
    fn inner_ref<'a>(a: &'a Pin<ReadResult>) -> &'a [u8] {a}

    let bufbox = Box::into_raw(Box::new(Pin::new(buf)));
    let r = {
        // SAFETY: The references will not be referenced after this scope
        let bufref = BullRef(inner_ref(unsafe{&*bufbox}) as *const [u8]);
        let r = executor().spawn_blocking(move || {
            let bufref = bufref;
            func(unsafe{&*bufref.0})
        }).await;
        
        r
    };
    //SAFETY: All closures are FnOnce
    let bufbox = unsafe{Box::from_raw(bufbox)};
    (Pin::into_inner(*bufbox),r)
}

fn fadvise_sequential(f: &impl AsRawFd) {
    unsafe {
        assert_eq!(
            libc::posix_fadvise(
                f.as_raw_fd(),
                0,0,
                libc::POSIX_FADV_SEQUENTIAL,
            ),
            0
        );
    }
}
