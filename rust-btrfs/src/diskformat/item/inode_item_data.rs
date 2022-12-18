use std::fmt::Debug;
use std::fmt::DebugStruct;
use std::fmt::Error as FmtError;
use std::fmt::Formatter;

use super::super::*;

#[repr(C, packed)]
#[derive(Copy, Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BtrfsInodeItemData {
    pub generation: u64,
    pub transaction_id: u64,
    pub st_size: u64,
    pub st_blocks: u64,
    pub block_group: u64,
    pub st_nlink: u32,
    pub st_uid: u32,
    pub st_gid: u32,
    pub st_mode: u32,
    pub st_rdev: u64,
    pub flags: u64,
    pub sequence: u64,
    pub reserved: [u8; 0x20],
    pub st_atime: BtrfsTimestamp,
    pub st_ctime: BtrfsTimestamp,
    pub st_mtime: BtrfsTimestamp,
    pub st_otime: BtrfsTimestamp,
}

impl BtrfsInodeItemData {
    pub fn debug_struct(&self, debug_struct: &mut DebugStruct) {
        let Self {
            generation,
            transaction_id,
            st_size,
            st_blocks,
            block_group,
            st_nlink,
            st_uid,
            st_gid,
            st_mode,
            st_rdev,
            flags,
            sequence,
            st_atime,
            st_ctime,
            st_mtime,
            st_otime,
            ..
        } = *self;

        debug_struct.field("generation", &generation);

        debug_struct.field("transaction_id", &transaction_id);

        debug_struct.field("st_size", &st_size);

        debug_struct.field("st_blocks", &st_blocks);

        debug_struct.field("block_group", &block_group);

        debug_struct.field("st_nlink", &st_nlink);

        debug_struct.field("st_uid", &st_uid);

        debug_struct.field("st_gid", &st_gid);

        debug_struct.field(
            "st_mode",
            &NakedString::from(format!("0o{st_mode:5o}")),
        );

        debug_struct.field("st_rdev", &st_rdev);

        debug_struct.field("flags", &flags);

        debug_struct.field("sequence", &sequence);

        debug_struct.field("st_atime", &NakedString::from(st_atime.to_string()));

        debug_struct.field("st_ctime", &NakedString::from(st_ctime.to_string()));

        debug_struct.field("st_mtime", &NakedString::from(st_mtime.to_string()));

        debug_struct.field("st_otime", &NakedString::from(st_otime.to_string()));
    }
}

impl Debug for BtrfsInodeItemData {
    fn fmt(&self, formatter: &mut Formatter) -> Result<(), FmtError> {
        let mut debug_struct = formatter.debug_struct("BtrfsInodeItemData");

        self.debug_struct(&mut debug_struct);

        debug_struct.finish()
    }
}

#[cfg(test)]
mod tests {

    use std::mem;

    use super::*;

    #[test]
    fn test_size() {
        assert!(mem::size_of::<BtrfsInodeItemData>() == 0xa0);
    }
}
