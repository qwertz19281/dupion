use super::*;
use util::*;
use vfs::{VfsId, Vfs, entry::VfsEntryType};
use std::{collections::{HashMap, hash_map::Entry}, sync::Arc};
use group::{HashGroup, SizeGroup};
use opts::Opts;

pub struct State {
    pub tree: Vfs,
    pub sizes: Sizes,
    pub hashes: Hashes,
    pub cache_allowed: bool,
}

impl State {
    pub fn push_to_size_group(&mut self, id: VfsId, file: bool, dir: bool) -> AnyhowResult<()> {
        ensure!(self.tree[id].valid,"Attemped to group non-validated entry");
        if !self.tree[id].unique {
            ensure!(!file || self.tree[id].is_file || self.tree[id].file_size.is_some(),"The to push entry to SizeTable has no size (file)");
            ensure!(!dir || self.tree[id].is_dir || self.tree[id].dir_size.is_some(),"The to push entry to SizeTable has no size (dir)");
            
            fn infuse(senf: &mut State, id: VfsId, size: Size, typ: VfsEntryType) -> AnyhowResult<()> {
                match senf.sizes.entry(size) {
                    Entry::Vacant(v) => {
                        v.insert(SizeGroup{
                            entries: vec![(typ,id)],
                            size,
                        });
                    }
                    Entry::Occupied(mut v) => {
                        let v = v.get_mut();
                        assert_eq!(v.size,size);
                        v.entries.retain(|e| e != &(typ,id) ); //TODO opti
                        v.entries.push((typ,id));
                    }
                }
                Ok(())
            }
            
            if file {
                infuse(self,id,self.tree[id].file_size.unwrap(),VfsEntryType::File)?;
            }
            if dir {
                infuse(self,id,self.tree[id].dir_size.unwrap(),VfsEntryType::Dir)?;
            }
        }
        Ok(())
    }
    pub fn push_to_hash_group(&mut self, id: VfsId, file: bool, dir: bool) -> AnyhowResult<()> {
        ensure!(self.tree[id].valid,"Attemped to group non-validated entry");
        if !self.tree[id].unique {
            ensure!(!file || self.tree[id].is_file || self.tree[id].file_hash.is_some(),"The to push entry to HashTable has no hash (file)");
            ensure!(!dir || self.tree[id].is_dir || self.tree[id].dir_hash.is_some(),"The to push entry to HashTable has no hash (dir)");
            
            fn infuse(hashes: &mut Hashes, id: VfsId, size: Size, hash: &mut Hash, typ: VfsEntryType) -> AnyhowResult<()> {
                match hashes.entry(hash.clone()) {
                    Entry::Vacant(v) => {
                        v.insert(HashGroup{
                            entries: vec![(typ,id)],
                            size,
                            hash: hash.clone(),
                        });
                    }
                    Entry::Occupied(mut v) => {
                        let v = v.get_mut();
                        assert_eq!(v.size,size);
                        assert_eq!(v.hash,*hash);
                        *hash = Arc::clone(&v.hash); //arc clone dedup
                        v.entries.retain(|e| e != &(typ,id) ); //TODO opti
                        v.entries.push((typ,id));
                    }
                }
                Ok(())
            }
            
            if file {
                let size = self.tree[id].file_size.unwrap();
                infuse(&mut self.hashes,id,size,self.tree[id].file_hash.as_mut().unwrap(),VfsEntryType::File)?;
            }
            if dir {
                let size = self.tree[id].dir_size.unwrap();
                infuse(&mut self.hashes,id,size,self.tree[id].dir_hash.as_mut().unwrap(),VfsEntryType::Dir)?;
            }
        }
        Ok(())
    }
    pub fn more_than_one_size(&self, size: Size) -> bool {
        self.sizes.get(&size)
            .map_or(false, |e| e.entries.len() > 1)
    }
    pub fn is_file_read_candidate(&self, id: VfsId, opts: &Opts) -> bool {
        let mut do_hash = true;
        //only hash if no hash
        do_hash &= self.tree[id].file_hash.is_none();
        //only files
        do_hash &= self.tree[id].is_file;
        //only hash if non-unique size or possible archive
        do_hash &= self.more_than_one_size(self.tree[id].file_size.unwrap()) || opts.zip_by_extension(&self.tree[id].path);
        //hash anyway if possible archive and not dir (happens if archive-scanning was disabled in previous cache)
        do_hash |= opts.zip_by_extension(&self.tree[id].path) && self.tree[id].is_file && !self.tree[id].is_dir;
        do_hash
    }
    pub fn more_than_one_hash(&self, hash: &Hash) -> bool {
        self.num_hashes(hash) > 1
    }
    pub fn num_hashes(&self, hash: &Hash) -> usize {
        self.hashes.get(hash)
            .map_or(0, |e| e.entries.len() )
    }
    pub fn new(cache_allowed: bool) -> Self {
        Self{
            tree: Vfs::new(),
            sizes: HashMap::with_capacity(16384),
            hashes: HashMap::with_capacity(16384),
            cache_allowed,
        }
    }
}
