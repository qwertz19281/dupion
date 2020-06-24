use super::*;
use std::{collections::HashMap, sync::Arc};
use sha2::digest::generic_array::GenericArray;
use sha2::digest::generic_array::typenum::U64;
use group::{SizeGroup, HashGroup};
use std::{io::{Seek, Read}, sync::{atomic::{Ordering, AtomicUsize, AtomicBool}}, time::Duration, ops::{DerefMut, Deref}, mem::ManuallyDrop};
use parking_lot::{RawMutex, Mutex};
use parking_lot::lock_api::RawMutex as _;

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
        let x = self.mutex.lock();
        let r = self.inner.read_vectored(bufs);
        drop(x);
        r
    }
    fn read_to_end(&mut self, buf: &mut Vec<u8>) -> std::io::Result<usize> {
        let x = self.mutex.lock();
        let r = self.inner.read_to_end(buf);
        drop(x);
        r
    }
    fn read_to_string(&mut self, buf: &mut String) -> std::io::Result<usize> {
        let x = self.mutex.lock();
        let r = self.inner.read_to_string(buf);
        drop(x);
        r
    }
    fn read_exact(&mut self, buf: &mut [u8]) -> std::io::Result<()> {
        let x = self.mutex.lock();
        let r = self.inner.read_exact(buf);
        drop(x);
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
        let x = self.mutex.lock();
        let r = self.inner.read(buf);
        drop(x);
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
