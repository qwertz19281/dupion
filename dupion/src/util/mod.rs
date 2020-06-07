use super::*;
use std::{collections::HashMap, sync::Arc};
use sha2::digest::generic_array::GenericArray;
use sha2::digest::generic_array::typenum::U64;
use group::{SizeGroup, HashGroup};
use std::{io::{Seek, Read}, sync::{Mutex, atomic::{Ordering, AtomicUsize, AtomicBool}}, time::Duration, ops::{DerefMut, Deref}};

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

pub struct MutexedReader<'m,R> {
    pub inner: R,
    pub mutex: &'m Mutex<()>,
}

impl<'m,R> Read for MutexedReader<'m,R> where R: Read {
    fn read_vectored(&mut self, bufs: &mut [std::io::IoSliceMut<'_>]) -> std::io::Result<usize> {
        let _ = self.mutex.lock().unwrap();
        self.inner.read_vectored(bufs)
    }
    fn read_to_end(&mut self, buf: &mut Vec<u8>) -> std::io::Result<usize> {
        let _ = self.mutex.lock().unwrap();
        self.inner.read_to_end(buf)
    }
    fn read_to_string(&mut self, buf: &mut String) -> std::io::Result<usize> {
        let _ = self.mutex.lock().unwrap();
        self.inner.read_to_string(buf)
    }
    fn read_exact(&mut self, buf: &mut [u8]) -> std::io::Result<()> {
        let _ = self.mutex.lock().unwrap();
        self.inner.read_exact(buf)
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
        let _ = self.mutex.lock().unwrap();
        self.inner.read(buf)
    }

}
impl<'m,R> Seek for MutexedReader<'m,R> where R: Seek {
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
