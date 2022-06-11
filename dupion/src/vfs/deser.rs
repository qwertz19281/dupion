use super::*;

use base64::decode_config_slice;
use rustc_hash::FxHasher;
use serde::de::{Visitor, SeqAccess};
use serde::{Serialize, Serializer, Deserialize, Deserializer};
use serde_derive::*;
use std::borrow::Cow;
use std::hash::BuildHasherDefault;
use std::{io::BufReader, sync::atomic::Ordering};
use state::State;
use util::{vfs_store_notif, Hash, Size};
use std::fs::File;

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
        let i = EntryIntermediate::from_entry(self);

        i.serialize(serializer)
    }
}

impl<'a> EntryIntermediate<'a> {
    fn from_entry(entry: &'a VfsEntry) -> Self {
        let file_hash = entry.file_hash.as_ref().map(encode_hash);
        EntryIntermediate {
            path: Cow::Borrowed(entry.path.to_str().unwrap()),
            ctime: entry.ctime,
            file_size: entry.file_size,
            file_hash: file_hash.map(|a| Cow::Owned(a) ),
            childs: entry.childs.clone(),
            was_file: entry.is_file || (entry.was_file && !entry.valid),
            was_dir: entry.is_dir || (entry.was_dir && !entry.valid),
            upgrade: entry.failure,
            dedup_state: entry.dedup_state,
            phys: entry.phys,
        }
    }

    fn into_entry(self, interner: &mut InternSet) -> VfsEntry {
        let path: Arc<Path> = PathBuf::from(self.path.as_ref()).into();

        VfsEntry{
            plc: to_plc(&path),
            path,
            ctime: self.ctime,
            file_size: self.file_size,
            dir_size: None,
            file_hash: self.file_hash.map(|h| decode_and_intern_hash(&h, interner) ),
            dir_hash: None,
            childs: self.childs,
            valid: false,
            is_file: false,
            is_dir: false,
            was_file: self.was_file,
            was_dir: self.was_dir,
            file_shadowed: false,
            dir_shadowed: false,
            unique: false,
            disp_relevated: false,
            failure: self.upgrade,
            treediff_stat: 0,
            dedup_state: self.dedup_state,
            phys: None,
        }
    }
}

struct VfsEntries(Vec<VfsEntry>);

impl Serialize for VfsEntries {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error> where S: Serializer {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for VfsEntries {
    fn deserialize<D>(deserializer: D) -> Result<VfsEntries, D::Error> where D: Deserializer<'de> {
        struct VfsEntryVisitor {
            interner: InternSet,
        }

        impl<'de> Visitor<'de> for VfsEntryVisitor {
            type Value = VfsEntries;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a sequence")
            }

            fn visit_seq<A>(mut self, mut seq: A) -> Result<Self::Value, A::Error> where A: SeqAccess<'de> {
                let mut entries = Vec::with_capacity(16384);

                while let Some(value) = seq.next_element::<EntryIntermediate>()? {
                    entries.push(value.into_entry(&mut self.interner));
                }

                Ok(VfsEntries(entries))
            }
        }

        let visitor = VfsEntryVisitor {
            interner: hashbrown::HashSet::with_capacity_and_hasher(16384,Default::default()),
        };

        deserializer.deserialize_seq(visitor)
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
                let VfsEntries(entries) = tryz!(serde_json::from_reader(reader));
                self.tree.entries = entries;
            }
        }
    }
}

pub fn encode_hash(h: &Hash) -> String {
    base64::encode(&h[..])
}

type InternSet = hashbrown::HashSet<Hash,BuildHasherDefault<FxHasher>>;

pub fn decode_and_intern_hash(h: &str, interner: &mut InternSet) -> Hash {
    let mut decoded = [0u8;32];
    assert_eq!(
        decode_config_slice(h, base64::STANDARD, &mut decoded).unwrap(),
        decoded.len()
    );

    interner.get_or_insert_with(&decoded, |v| {
        debug_assert_eq!(v, &decoded);
        Arc::new(decoded)
    }).clone()
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
