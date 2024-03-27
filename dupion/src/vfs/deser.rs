use crate::util::HASH_SIZE;

use super::*;

use base64::Engine;
use rustc_hash::FxHasher;
use serde::de::{Visitor, SeqAccess};
use serde::ser::SerializeSeq;
use serde::{Serialize, Serializer, Deserialize, Deserializer};
use serde_bytes::ByteBuf;
use serde_derive::*;
use std::borrow::Cow;
use std::hash::BuildHasherDefault;
use std::io::BufRead;
use std::{io::BufReader, sync::atomic::Ordering};
use state::State;
use util::{VFS_STORE_NOTIF, Hash, Size};
use std::fs::File;

#[derive(Serialize,Deserialize)]
struct EntryIntermediateMsgPack<'a> {
    path: Cow<'a,str>,
    ctime: Option<i64>,
    file_size: Option<Size>,
    file_hash: Option<ByteBuf>,
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

#[derive(Deserialize)]
struct EntryIntermediateJson<'a> {
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

impl<'a> EntryIntermediateMsgPack<'a> {
    fn from_entry(entry: &'a VfsEntry) -> Self {
        let file_hash = entry.file_hash.as_deref().map(|v| ByteBuf::from(Vec::<u8>::from(&v[..])));
        Self {
            path: Cow::Borrowed(entry.path.to_str().unwrap()),
            ctime: entry.ctime,
            file_size: entry.file_size,
            file_hash,
            childs: entry.childs.clone(),
            was_file: entry.is_file || (entry.was_file && !entry.valid),
            was_dir: entry.is_dir || (entry.was_dir && !entry.valid),
            upgrade: entry.failure,
            dedup_state: entry.dedup_state,
            phys: entry.phys,
        }
    }

    fn into_entry(self, interner: &mut InternSet) -> anyhow::Result<VfsEntry> {
        let path: Arc<Path> = PathBuf::from(self.path.as_ref()).into();

        Ok(VfsEntry {
            plc: to_plc(&path),
            path,
            ctime: self.ctime,
            file_size: self.file_size,
            dir_size: None,
            file_hash: self.file_hash.and_then(|h| intern_hash_raw(&h, interner).transpose() ).transpose()?,
            dir_hash: None,
            childs2: rustc_hash::FxHashMap::with_capacity_and_hasher(self.childs.len(), Default::default()),
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
            n_extends: None,
        })
    }
}

impl<'a> EntryIntermediateJson<'a> {
    fn into_entry(self, interner: &mut InternSet) -> anyhow::Result<VfsEntry> {
        let path: Arc<Path> = PathBuf::from(self.path.as_ref()).into();

        Ok(VfsEntry {
            plc: to_plc(&path),
            path,
            ctime: self.ctime,
            file_size: self.file_size,
            dir_size: None,
            file_hash: self.file_hash.and_then(|h| decode_and_intern_hash_base64(&h, interner).transpose() ).transpose()?,
            dir_hash: None,
            childs2: rustc_hash::FxHashMap::with_capacity_and_hasher(self.childs.len(), Default::default()),
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
            n_extends: None,
        })
    }
}

struct VfsEntriesMsgPack(Vec<VfsEntry>);
struct VfsEntriesJson(Vec<VfsEntry>);

impl<'de> Deserialize<'de> for VfsEntriesMsgPack {
    fn deserialize<D>(deserializer: D) -> Result<VfsEntriesMsgPack, D::Error> where D: Deserializer<'de> {
        struct VfsEntryVisitor {
            interner: InternSet,
        }

        impl<'de> Visitor<'de> for VfsEntryVisitor {
            type Value = VfsEntriesMsgPack;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a sequence")
            }

            fn visit_seq<A>(mut self, mut seq: A) -> Result<Self::Value, A::Error> where A: SeqAccess<'de> {
                let mut entries = Vec::with_capacity(
                    seq.size_hint().map_or(16384, |s| s - 1 )
                );

                if !seq.next_element::<CacheHeader>()?.is_some_and(|h| h.version == 4 ) {
                    return Err(serde::de::Error::custom("Version not 4"));
                }

                while let Some(value) = seq.next_element::<EntryIntermediateMsgPack>()? {
                    entries.push(value.into_entry(&mut self.interner).map_err(serde::de::Error::custom)?);
                }

                Ok(VfsEntriesMsgPack(entries))
            }
        }

        let visitor = VfsEntryVisitor {
            interner: hashbrown::HashSet::with_capacity_and_hasher(16384,Default::default()),
        };

        deserializer.deserialize_seq(visitor)
    }
}

impl<'de> Deserialize<'de> for VfsEntriesJson {
    fn deserialize<D>(deserializer: D) -> Result<VfsEntriesJson, D::Error> where D: Deserializer<'de> {
        struct VfsEntryVisitor {
            interner: InternSet,
        }

        impl<'de> Visitor<'de> for VfsEntryVisitor {
            type Value = VfsEntriesJson;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a sequence")
            }

            fn visit_seq<A>(mut self, mut seq: A) -> Result<Self::Value, A::Error> where A: SeqAccess<'de> {
                let mut entries = Vec::with_capacity(16384);

                while let Some(value) = seq.next_element::<EntryIntermediateJson>()? {
                    entries.push(value.into_entry(&mut self.interner).map_err(serde::de::Error::custom)?);
                }

                Ok(VfsEntriesJson(entries))
            }
        }

        let visitor = VfsEntryVisitor {
            interner: hashbrown::HashSet::with_capacity_and_hasher(16384,Default::default()),
        };

        deserializer.deserialize_seq(visitor)
    }
}

#[derive(Serialize,Deserialize)]
struct CacheHeader {
    version: usize,
}

struct VfsEntriesSerialize<'a>(&'a [VfsEntry]);

impl Serialize for VfsEntriesSerialize<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut serializer = serializer.serialize_seq(Some(self.0.len()+1))?;

        serializer.serialize_element(&CacheHeader { version: 4 })?;

        for entry in self.0 {
            serializer.serialize_element(&EntryIntermediateMsgPack::from_entry(&entry))?;
        }

        serializer.end()
    }
}

impl State {
    pub fn eventually_store_vfs(&self, path: &Path, force: bool) {
        self.try_eventually_store_vfs(path, force).unwrap_or_else(|e| dprintln!("Error writing cache: {e}") )
    }

    pub fn try_eventually_store_vfs(&self, path: &Path, force: bool) -> anyhow::Result<()> {
        if self.cache_allowed && (force || VFS_STORE_NOTIF.swap(false,Ordering::Relaxed)) {
            let mut stor = Vec::with_capacity(1024*1024);
            let mut writer = zstd::stream::write::Encoder::new(&mut stor, 3)?;
            let mut ser = rmp_serde::Serializer::new(&mut writer).with_struct_map();
            VfsEntriesSerialize(&self.tree.entries).serialize(&mut ser)?;
            writer.finish()?;
            std::fs::write(path,&stor)?;
            //dprintln!("Wrote cache");
        }
        Ok(())
    }

    pub fn eventually_load_vfs(&mut self, path: &Path) {
        self.try_eventually_load_vfs(path).unwrap_or_else(|e| dprintln!("Error reading cache: {e}") );
    }

    pub fn try_eventually_load_vfs(&mut self, path: &Path) -> anyhow::Result<()> {
        if self.cache_allowed {
            let path_meta = path.metadata()?;
            if path_meta.is_file() {
                let reader = File::open(&path)?;

                let buf_reader_size = zstd::zstd_safe::DCtx::in_size() * 8;

                let mut buf_reader = BufReader::with_capacity(buf_reader_size, reader);

                if buf_reader.fill_buf()?.starts_with(&ZSTD_MAGIC_NUMBER) {
                    let reader = zstd::stream::read::Decoder::with_buffer(buf_reader)?;
                    let VfsEntriesMsgPack(entries) = rmp_serde::from_read(reader)?;
                    self.tree.entries = entries;
                } else {
                    let VfsEntriesJson(entries) = serde_json::from_reader(buf_reader)?;
                    self.tree.entries = entries;
                }

                for i in 0 .. self.tree.entries.len() {
                    let i = VfsId { evil_inner: i };
                    if self.tree[i].childs2.is_empty() && !self.tree[i].childs.is_empty() {
                        for j in 0 .. self.tree[i].childs.len() {
                            let id = self.tree[i].childs[j];
                            let plc = self.tree[id].plc.clone();
                            self.tree[i].childs2.insert(plc, id);
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

const ZSTD_MAGIC_NUMBER: [u8;4] = 0xFD2F_B528_u32.to_le_bytes();

const BASE64_ENGINE: base64::engine::GeneralPurpose = base64::engine::GeneralPurpose::new(
    &base64::alphabet::STANDARD,
    base64::engine::general_purpose::GeneralPurposeConfig::new()
        .with_encode_padding(true)
        .with_decode_allow_trailing_bits(true)
        .with_decode_padding_mode(base64::engine::DecodePaddingMode::Indifferent)
);

type InternSet = hashbrown::HashSet<Hash,BuildHasherDefault<FxHasher>>;

const BASE64_BUF_IN: usize = 44;
const BASE64_BUF_BUF: usize = 64;
const _: () = assert!(BASE64_BUF_BUF >= HASH_SIZE);

pub fn decode_and_intern_hash_base64(h: &str, interner: &mut InternSet) -> anyhow::Result<Option<Hash>> {
    // Discard old sha512 hashes
    if h.len() > BASE64_BUF_IN {return Ok(None);}

    let mut decoded = [0u8;BASE64_BUF_BUF];
    anyhow::ensure!(
        BASE64_ENGINE.decode_slice(h, &mut decoded)? == HASH_SIZE
    );

    let mut truncated = [0u8;HASH_SIZE];

    for i in 0 .. HASH_SIZE {
        truncated[i] = decoded[i];
    }

    Ok(Some(
        interner.get_or_insert_with(&truncated, |v| {
            debug_assert_eq!(v, &truncated);
            Arc::new(truncated)
        })
        .clone()
    ))
}

pub fn intern_hash_raw(h: &[u8], interner: &mut InternSet) -> anyhow::Result<Option<Hash>> {
    if h.len() != HASH_SIZE {bail!("Invalid hash length");}

    let mut truncated = [0u8;HASH_SIZE];

    for i in 0 .. HASH_SIZE {
        truncated[i] = h[i];
    }

    Ok(Some(
        interner.get_or_insert_with(&truncated, |v| {
            debug_assert_eq!(v, &truncated);
            Arc::new(truncated)
        })
        .clone()
    ))
}