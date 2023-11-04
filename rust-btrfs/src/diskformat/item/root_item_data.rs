use std::fmt::Debug;
use std::fmt::DebugStruct;
use std::fmt::Error as FmtError;
use std::fmt::Formatter;

use super::super::*;

#[repr(C, packed)]
#[derive(Copy, Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BtrfsRootItemData {
    pub inode_item: BtrfsInodeItemData,
    pub expected_generation: u64,
    pub root_object_id: u64,
    pub root_node_block_number: u64,
    pub byte_limit: u64,
    pub bytes_used: u64,
    pub last_snapshot_generation: u64,
    pub flags: u64,
    pub num_references: u32,
    pub drop_progress: BtrfsKey,
    pub drop_level: u8,
    pub tree_level: u8,
    pub generation_v2: u64,
    pub subvolume_uuid: BtrfsUuid,
    pub parent_uuid: BtrfsUuid,
    pub received_uuid: BtrfsUuid,
    pub changed_transaction_id: u64,
    pub created_transaction_id: u64,
    pub sent_transaction_id: u64,
    pub received_transaction_id: u64,
    pub changed_time: BtrfsTimestamp,
    pub created_time: BtrfsTimestamp,
    pub sent_time: BtrfsTimestamp,
    pub received_time: BtrfsTimestamp,
    pub reserved: [u64; 0x8],
}

impl BtrfsRootItemData {
    pub fn debug_struct(&self, debug_struct: &mut DebugStruct) {
        let Self {
            inode_item,
            expected_generation,
            root_object_id,
            root_node_block_number,
            byte_limit,
            bytes_used,
            last_snapshot_generation,
            flags,
            num_references,
            drop_progress,
            drop_level,
            tree_level,
            generation_v2,
            subvolume_uuid,
            parent_uuid,
            received_uuid,
            changed_transaction_id,
            created_transaction_id,
            sent_transaction_id,
            received_transaction_id,
            changed_time,
            created_time,
            sent_time,
            received_time,
            ..
        } = *self;

        debug_struct.field("inode_item", &inode_item);

        debug_struct.field("expected_generation", &expected_generation);

        debug_struct.field("root_object_id", &root_object_id);

        debug_struct.field("root_node_block_number", &root_node_block_number);

        debug_struct.field("byte_limit", &byte_limit);

        debug_struct.field("bytes_used", &bytes_used);

        debug_struct.field("last_snapshot_generation", &last_snapshot_generation);

        debug_struct.field("flags", &flags);

        debug_struct.field("num_references", &num_references);

        debug_struct.field(
            "drop_progress",
            &NakedString::from(drop_progress.to_string()),
        );

        debug_struct.field("drop_level", &drop_level);

        debug_struct.field("tree_level", &tree_level);

        debug_struct.field("generation_v2", &generation_v2);

        debug_struct.field(
            "subvolume_uuid",
            &NakedString::from(subvolume_uuid.to_string()),
        );

        debug_struct.field(
            "parent_uuid",
            &NakedString::from(parent_uuid.to_string()),
        );

        debug_struct.field(
            "received_uuid",
            &NakedString::from(received_uuid.to_string()),
        );

        debug_struct.field("changed_transaction_id", &changed_transaction_id);

        debug_struct.field("created_transaction_id", &created_transaction_id);

        debug_struct.field("sent_transaction_id", &sent_transaction_id);

        debug_struct.field("received_transaction_id", &received_transaction_id);

        debug_struct.field(
            "changed_time",
            &NakedString::from(changed_time.to_string()),
        );

        debug_struct.field(
            "created_time",
            &NakedString::from(created_time.to_string()),
        );

        debug_struct.field("sent_time", &NakedString::from(sent_time.to_string()));

        debug_struct.field(
            "received_time",
            &NakedString::from(received_time.to_string()),
        );
    }
}

impl Debug for BtrfsRootItemData {
    fn fmt(&self, formatter: &mut Formatter) -> Result<(), FmtError> {
        let mut debug_struct = formatter.debug_struct("BtrfsRootItemData");

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
        assert!(mem::size_of::<BtrfsRootItemData>() == 0x1b7);
    }
}
