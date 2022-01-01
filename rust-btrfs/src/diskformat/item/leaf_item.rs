use super::super::*;

#[derive(Clone, Debug)]
pub enum BtrfsLeafItem<'a> {
    ChunkItem(BtrfsChunkItem<'a>),
    DirIndex(BtrfsDirIndex<'a>),
    DirItem(BtrfsDirItem<'a>),
    ExtentData(BtrfsExtentData<'a>),
    ExtentItem(BtrfsExtentItem<'a>),
    InodeItem(BtrfsInodeItem<'a>),
    InodeRef(BtrfsInodeRef<'a>),
    Invalid(BtrfsInvalidItem<'a>),
    RootBackref(BtrfsRootBackref<'a>),
    RootItem(BtrfsRootItem<'a>),
    RootRef(BtrfsRootRef<'a>),
    Unknown(BtrfsUnknownItem<'a>),
}

impl<'a> BtrfsLeafItem<'a> {
    pub fn from_bytes(header: &'a BtrfsLeafItemHeader, data_bytes: &'a [u8]) -> BtrfsLeafItem<'a> {
        match header.key().item_type() {
            BTRFS_CHUNK_ITEM_TYPE => {
                BtrfsChunkItem::from_bytes(header, data_bytes).map(BtrfsLeafItem::ChunkItem)
            }

            BTRFS_DIR_INDEX_TYPE => {
                BtrfsDirIndex::from_bytes(header, data_bytes).map(BtrfsLeafItem::DirIndex)
            }

            BTRFS_DIR_ITEM_TYPE => {
                BtrfsDirItem::from_bytes(header, data_bytes).map(BtrfsLeafItem::DirItem)
            }

            BTRFS_EXTENT_DATA_TYPE => {
                BtrfsExtentData::from_bytes(header, data_bytes).map(BtrfsLeafItem::ExtentData)
            }

            BTRFS_EXTENT_ITEM_TYPE => {
                BtrfsExtentItem::from_bytes(header, data_bytes).map(BtrfsLeafItem::ExtentItem)
            }

            BTRFS_INODE_ITEM_TYPE => {
                BtrfsInodeItem::from_bytes(header, data_bytes).map(BtrfsLeafItem::InodeItem)
            }

            BTRFS_INODE_REF_TYPE => {
                BtrfsInodeRef::from_bytes(header, data_bytes).map(BtrfsLeafItem::InodeRef)
            }

            BTRFS_ROOT_BACKREF_ITEM_TYPE => {
                BtrfsRootBackref::from_bytes(header, data_bytes).map(BtrfsLeafItem::RootBackref)
            }

            BTRFS_ROOT_ITEM_TYPE => {
                BtrfsRootItem::from_bytes(header, data_bytes).map(BtrfsLeafItem::RootItem)
            }

            BTRFS_ROOT_REF_ITEM_TYPE => {
                BtrfsRootRef::from_bytes(header, data_bytes).map(BtrfsLeafItem::RootRef)
            }

            _ => Ok(BtrfsLeafItem::Unknown(BtrfsUnknownItem::new(
                header, data_bytes,
            ))),
        }
        .unwrap_or_else(|error| {
            BtrfsLeafItem::Invalid(BtrfsInvalidItem::new(header, data_bytes, error))
        })
    }

    pub fn contents(&self) -> &dyn BtrfsLeafItemContents<'_> {
        match self {
            BtrfsLeafItem::ChunkItem(chunk_item) => chunk_item,
            BtrfsLeafItem::DirIndex(dir_index) => dir_index,
            BtrfsLeafItem::DirItem(dir_item) => dir_item,
            BtrfsLeafItem::ExtentData(extent_data) => extent_data,
            BtrfsLeafItem::ExtentItem(extent_item) => extent_item,

            BtrfsLeafItem::InodeItem(inode_item) => inode_item,
            BtrfsLeafItem::InodeRef(inode_ref) => inode_ref,
            BtrfsLeafItem::Invalid(invalid_item) => invalid_item,
            BtrfsLeafItem::RootBackref(root_backref) => root_backref,
            BtrfsLeafItem::RootItem(root_item) => root_item,
            BtrfsLeafItem::RootRef(root_ref) => root_ref,
            BtrfsLeafItem::Unknown(unknown_item) => unknown_item,
        }
    }

    pub fn header(&self) -> &BtrfsLeafItemHeader {
        self.contents().header()
    }

    pub fn key(&self) -> BtrfsKey {
        self.header().key()
    }

    pub fn object_id(&self) -> u64 {
        self.contents().object_id()
    }

    pub fn item_type(&self) -> u8 {
        self.contents().item_type()
    }

    pub fn offset(&self) -> u64 {
        self.contents().offset()
    }

    pub fn as_root_item(&'a self) -> Option<&'a BtrfsRootItem<'a>> {
        match self {
            BtrfsLeafItem::RootItem(item) => Some(item),
            _ => None,
        }
    }
} // ex: noet ts=4 filetype=rust
