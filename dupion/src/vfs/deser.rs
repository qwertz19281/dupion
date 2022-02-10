use super::*;

use base64::decode_config_slice;
use rustc_hash::FxHashSet;
use serde::{Serialize, Serializer, Deserialize, Deserializer};
use serde_derive::*;
use std::borrow::Cow;
use std::{io::BufReader, sync::atomic::Ordering, collections::HashSet};
use sha2::digest::generic_array::GenericArray;
use state::State;
use util::{vfs_store_notif, Hash, Size};
use std::{fs::File, cell::RefCell};

#[derive(Serialize,Deserialize)]
struct EntryIntermediate<'a> {
    path: Cow<'a,str>,
    ctime: Option<i64>,
    file_size: Option<Size>,
    file_hash: Option<Cow<'a,str>>,
    childs: Vec<VfsId>,
    was_file: bool,
    was_dir: bool,
    ///use for libarchive fail, so if set and number smaller than current version, force rehash
    #[serde(default)] 
    upgrade: Option<u64>,
    #[serde(default)] 
    dedup_state: Option<bool>,
    #[serde(default)] 
    phys: Option<u64>,
}

impl Serialize for VfsEntry {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let file_hash = self.file_hash.as_ref().map(encode_hash);
        let i = EntryIntermediate{
            path: Cow::Borrowed(self.path.to_str().unwrap()),
            ctime: self.ctime,
            file_size: self.file_size,
            file_hash: file_hash.as_deref().map(|a| Cow::Borrowed(a) ),
            childs: self.childs.clone(),
            was_file: self.is_file || (self.was_file && !self.valid),
            was_dir: self.is_dir || (self.was_dir && !self.valid),
            upgrade: self.failure,
            dedup_state: self.dedup_state,
            phys: self.phys,
        };
        i.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for VfsEntry {
    fn deserialize<D>(deserializer: D) -> Result<VfsEntry, D::Error>
    where
        D: Deserializer<'de>,
    {
        EntryIntermediate::deserialize(deserializer)
            .map(|i| {
                let path: Arc<Path> = PathBuf::from(i.path.as_ref()).into();
                VfsEntry{
                    plc: to_plc(&path),
                    path,
                    ctime: i.ctime,
                    file_size: i.file_size,
                    dir_size: None,
                    file_hash: i.file_hash.map(|h| decode_hash(&h) ),
                    dir_hash: None,
                    childs: i.childs,
                    valid: false,
                    is_file: false,
                    is_dir: false,
                    was_file: i.was_file,
                    was_dir: i.was_dir,
                    file_shadowed: false,
                    dir_shadowed: false,
                    unique: false,
                    disp_relevated: false,
                    failure: i.upgrade,
                    treediff_stat: 0,
                    dedup_state: i.dedup_state,
                    phys: None,
                }
            })
    }
}

impl State {
    pub fn eventually_store_vfs(&self, force: bool) {
        if self.cache_allowed && (force || vfs_store_notif.swap(false,Ordering::Relaxed)) {
            let mut stor = Vec::with_capacity(1024*1024);
            tryz!(serde_json::to_writer(&mut stor, &self.tree.entries));
            tryz!(std::fs::write("./dupion_cache",&stor));
            //eprintln!("Wrote cache");
        }
    }
    pub fn eventually_load_vfs(&mut self) {
        if self.cache_allowed {
            let path = PathBuf::from("./dupion_cache");
            if path.is_file() {
                let reader= tryz!(File::open(&path));
                let reader = BufReader::new(reader);
                let entries: Vec<VfsEntry> = tryz!(serde_json::from_reader(reader));
                self.tree.entries = entries;
                //drop the previous intern map
                DEDUP.with(|z|
                    *z.borrow_mut() = HashSet::with_capacity_and_hasher(0,Default::default())
                );
            }
        }
    }
}

pub fn encode_hash(h: &Hash) -> String {
    base64::encode(&***h)
}

//for Hash interning
thread_local! {
    pub static DEDUP: RefCell<FxHashSet<Hash>> = RefCell::new(FxHashSet::with_capacity_and_hasher(16384,Default::default()));
}

pub fn decode_hash(h: &str) -> Hash {
    let mut decoded = GenericArray::default();
    assert_eq!(
        decode_config_slice(h, base64::STANDARD, &mut decoded).unwrap(),
        decoded.len()
    );
    DEDUP.with(move |z| { 
        let mut z = z.borrow_mut();
        if let Some(v) = z.get(&decoded) {
            v.clone()
        }else{
            let arc = Arc::new(decoded);
            z.insert(arc.clone());
            arc
        }
    })
}

#[macro_export]
macro_rules! tryz {
    ($oof:expr) => {
        match $oof {
            Ok(f) => {
                f
            },
            Err(e) => {
                eprintln!("Error: {}",e);
                return;
            },
        }
    };
}

fn defhys() -> Option<u64> {
    Some(0)
}
