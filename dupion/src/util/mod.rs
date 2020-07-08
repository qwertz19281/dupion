use super::*;
use std::{collections::HashMap, sync::Arc};
use sha2::digest::generic_array::GenericArray;
use sha2::digest::generic_array::typenum::U64;
use group::{SizeGroup, HashGroup};
use std::{io::{Seek, Read}, sync::{atomic::{Ordering, AtomicUsize, AtomicBool}}, time::Duration, ops::{DerefMut, Deref}};
use parking_lot::RawMutex;
use parking_lot::lock_api::RawMutex as _;
use sysinfo::*;

pub type Size = u64;
pub type Hash = Arc<GenericArray<u8,U64>>;

pub type Sizes = HashMap<Size,SizeGroup>;
pub type Hashes = HashMap<Hash,HashGroup>;

pub static disp_found_bytes: AtomicUsize = AtomicUsize::new(0);
pub static disp_found_files: AtomicUsize = AtomicUsize::new(0);
pub static disp_relevant_bytes: AtomicUsize = AtomicUsize::new(0);
pub static disp_relevant_files: AtomicUsize = AtomicUsize::new(0);
pub static disp_processed_bytes: AtomicUsize = AtomicUsize::new(0);
pub static disp_processed_files: AtomicUsize = AtomicUsize::new(0);
pub static disp_deduped_bytes: AtomicUsize = AtomicUsize::new(usize::MAX);
pub static disp_prev: AtomicUsize = AtomicUsize::new(0);
pub static disp_enabled: AtomicBool = AtomicBool::new(false);
pub static vfs_store_notif: AtomicBool = AtomicBool::new(false);
pub static alloc_mon: AtomicUsize = AtomicUsize::new(0);

pub struct MutexedReader<R> {
    pub inner: R,
    pub mutex: ZeroLock,
}

impl<R> Read for MutexedReader<R> where R: Read {
    fn read_vectored(&mut self, bufs: &mut [std::io::IoSliceMut<'_>]) -> std::io::Result<usize> {
        self.mutex.lock();
        let r = self.inner.read_vectored(bufs);
        self.mutex.unlock();
        r
    }
    fn read_to_end(&mut self, buf: &mut Vec<u8>) -> std::io::Result<usize> {
        self.mutex.lock();
        let r = self.inner.read_to_end(buf);
        self.mutex.unlock();
        r
    }
    fn read_to_string(&mut self, buf: &mut String) -> std::io::Result<usize> {
        self.mutex.lock();
        let r = self.inner.read_to_string(buf);
        self.mutex.unlock();
        r
    }
    fn read_exact(&mut self, buf: &mut [u8]) -> std::io::Result<()> {
        self.mutex.lock();
        let r = self.inner.read_exact(buf);
        self.mutex.unlock();
        r
    }
    fn bytes(self) -> std::io::Bytes<Self>
    where
        Self: Sized,
    {
        panic!()
    }
    fn chain<S: Read>(self, _: S) -> std::io::Chain<Self, S>
    where
        Self: Sized,
    {
        panic!()
    }
    fn take(self, _: u64) -> std::io::Take<Self>
    where
        Self: Sized,
    {
        panic!()
    }
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.mutex.lock();
        let r = self.inner.read(buf);
        self.mutex.unlock();
        r
    }

}
impl<R> Seek for MutexedReader<R> where R: Seek {
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        self.inner.seek(pos)
    }
}

pub struct AllocMonBuf(Vec<u8>);

impl AllocMonBuf {
    pub fn new(size: usize, alloc_thresh: usize) -> Self {
        while alloc_mon.load(Ordering::Acquire)+size as usize > alloc_thresh {
            std::thread::sleep(Duration::from_millis(50));
        }
        let buf = vec![0;size as usize];
        assert_eq!(buf.len(),size);
        assert_eq!(buf.capacity(),size);
        alloc_mon.fetch_add(size as usize, Ordering::AcqRel);
        Self(buf)
    }
}

impl Deref for AllocMonBuf {
    type Target = Vec<u8>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl DerefMut for AllocMonBuf {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl Drop for AllocMonBuf {
    fn drop(&mut self) {
        alloc_mon.fetch_sub(self.capacity(), Ordering::AcqRel);
    }
}

pub struct ZeroLock {
    locked: bool,
    do_locking: bool,
    m: Arc<RawMutex>,
}

impl ZeroLock {
    pub fn new(do_locking: bool) -> Self{
        Self{
            locked: false,
            do_locking,
            m: Arc::new(RawMutex::INIT),
        }
    }
    pub fn clone(&self) -> Self {
        Self{
            locked: false,
            do_locking: self.do_locking,
            m: Arc::clone(&self.m),
        }
    }
    pub fn lock(&mut self) {
        if self.do_locking && !self.locked {
            self.m.lock();
            self.locked = true;
        }
    }
    pub fn unlock(&mut self) {
        if self.locked {
            unsafe{self.m.unlock();}
            self.locked = false;
        }
    }
}

impl Drop for ZeroLock {
    fn drop(&mut self) {
        self.unlock()
    }
}

pub struct CacheUsable {
    max: u64,
    sys: System,
}

impl CacheUsable {
    pub fn new(max: u64) -> Self {
        Self{
            max,
            sys: System::new(),
        }
    }

    pub fn get(&mut self) -> u64 {
        self.sys.refresh_memory();
        let sys_available = self.sys.get_total_memory() - self.sys.get_used_memory();
        let for_caching = (sys_available*1000/2+1024)/4096*4096;
        for_caching.min(self.max)
    }
}

/*pub trait PushGrow<T>: Extend<T> {
    fn reserve(&mut self, n: usize);
}

impl<T> PushGrow<T> for Vec<T> {
    fn reserve(&mut self, n: usize) {
        Vec::reserve(self,n)
    }
}

pub struct RopedVec<T> {
    inner: Vec<Vec<T>>,
}

impl<T> RopedVec<T> {
    pub fn new() -> Self {
        Self{
            inner: Vec::new(),
        }
    }

    pub fn push()
}

impl<T> Extend<T> for RopedVec<T> {
    fn extend<T: IntoIterator<Item = T>>(&mut self, iter: T) {
        todo!()
    }
}

impl<T> PushGrow<T> for RopedVec<T> {
    fn reserve(&mut self, n: usize) {
        todo!()
    }
}*/
