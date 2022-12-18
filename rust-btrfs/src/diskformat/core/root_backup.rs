use std::fmt::Debug;
use std::fmt::Error as FmtError;
use std::fmt::Formatter;

use super::super::*;

#[repr(C, packed)]
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct BtrfsRootBackup {
    pub tree_root: u64,
    pub tree_root_gen: u64,
    pub chunk_root: u64,
    pub chunk_root_gen: u64,
    pub extent_root: u64,
    pub extent_root_gen: u64,
    pub fs_root: u64,
    pub fs_root_gen: u64,
    pub dev_root: u64,
    pub dev_root_gen: u64,
    pub csum_root: u64,
    pub csum_root_gen: u64,
    pub total_bytes: u64,
    pub bytes_used: u64,
    pub num_devices: u64,
    pub unused_0: [u64; 4],
    pub tree_root_level: u8,
    pub chunk_root_level: u8,
    pub extent_root_level: u8,
    pub fs_root_level: u8,
    pub dev_root_level: u8,
    pub csum_root_level: u8,
    pub unused_1: [u8; 10],
}

impl Debug for BtrfsRootBackup {
    fn fmt(&self, formatter: &mut Formatter) -> Result<(), FmtError> {
        let Self {
            tree_root,
            tree_root_gen,
            chunk_root,
            chunk_root_gen,
            extent_root,
            extent_root_gen,
            fs_root,
            fs_root_gen,
            dev_root,
            dev_root_gen,
            csum_root,
            csum_root_gen,
            total_bytes,
            bytes_used,
            num_devices,
            tree_root_level,
            chunk_root_level,
            extent_root_level,
            fs_root_level,
            dev_root_level,
            csum_root_level,
            ..
        } = *self;

        let mut debug_struct = formatter.debug_struct("BtrfsRootBackup");

        debug_struct.field(
            "tree_root",
            &NakedString::from(format!("0x{tree_root:x}")),
        );

        debug_struct.field("tree_root_gen", &tree_root_gen);

        debug_struct.field(
            "chunk_root",
            &NakedString::from(format!("0x{chunk_root:x}")),
        );

        debug_struct.field("chunk_root_gen", &chunk_root_gen);

        debug_struct.field(
            "extent_root",
            &NakedString::from(format!("0x{extent_root:x}")),
        );

        debug_struct.field("extent_root_gen", &extent_root_gen);

        debug_struct.field(
            "fs_root",
            &NakedString::from(format!("0x{fs_root:x}")),
        );

        debug_struct.field("fs_root_gen", &fs_root_gen);

        debug_struct.field(
            "dev_root",
            &NakedString::from(format!("0x{dev_root:x}")),
        );

        debug_struct.field("dev_root_gen", &dev_root_gen);

        debug_struct.field(
            "csum_root",
            &NakedString::from(format!("0x{csum_root:x}")),
        );

        debug_struct.field("csum_root_gen", &csum_root_gen);

        debug_struct.field("total_bytes", &total_bytes);

        debug_struct.field("bytes_used", &bytes_used);

        debug_struct.field("num_devices", &num_devices);

        debug_struct.field("num_devices", &num_devices);

        debug_struct.field("tree_root_level", &tree_root_level);

        debug_struct.field("chunk_root_level", &chunk_root_level);

        debug_struct.field("extent_root_level", &extent_root_level);

        debug_struct.field("fs_root_level", &fs_root_level);

        debug_struct.field("dev_root_level", &dev_root_level);

        debug_struct.field("csum_root_level", &csum_root_level);

        debug_struct.finish()
    }
}

#[cfg(test)]
mod tests {

    use std::mem;

    use super::*;

    #[test]
    fn test_size() {
        assert!(mem::size_of::<BtrfsRootBackup>() == 0xa8);
    }
}
