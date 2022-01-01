use std::collections::BTreeMap;
use std::collections::HashMap;

use crate::diskformat::*;

pub type BtrfsChunkItemsByOffset<'a> = BTreeMap<u64, BtrfsChunkItem<'a>>;

pub struct BtrfsChunkTree<'a> {
    chunk_items_by_offset: BtrfsChunkItemsByOffset<'a>,
}

impl<'a> BtrfsChunkTree<'a> {
    pub fn new(devices: &'a BtrfsDeviceSet) -> Result<BtrfsChunkTree<'a>, String> {
        Ok(BtrfsChunkTree {
            chunk_items_by_offset: Self::read_system_extent_tree(devices)?
                .values()
                .filter_map(|extent_tree_item| {
                    if let BtrfsLeafItem::ChunkItem(chunk_item) = extent_tree_item {
                        Some((chunk_item.key().offset(), *chunk_item))
                    } else {
                        None
                    }
                })
                .collect(),
        })
    }

    fn read_system_extent_tree(
        devices: &'a BtrfsDeviceSet<'a>,
    ) -> Result<HashMap<BtrfsKey, BtrfsLeafItem<'a>>, String> {
        let mut extent_tree_items: HashMap<BtrfsKey, BtrfsLeafItem> = HashMap::new();

        Self::read_system_extent_tree_recurse(
            devices,
            devices.superblock().chunk_tree_logical_address(),
            &mut extent_tree_items,
        )?;

        Ok(extent_tree_items)
    }

    fn read_system_extent_tree_recurse(
        devices: &'a BtrfsDeviceSet<'a>,
        logical_address: u64,
        extent_tree_items: &mut HashMap<BtrfsKey, BtrfsLeafItem<'a>>,
    ) -> Result<(), String> {
        let node_physical_address =
            devices
                .system_logical_to_physical(logical_address)
                .ok_or(format!(
                    "Can't map logical address: 0x{:x}",
                    logical_address
                ))?;

        let node_bytes = devices.system_slice_at_logical_address(
            logical_address,
            devices.superblock().node_size() as usize,
        )?;

        let node = BtrfsNode::from_bytes(node_physical_address, node_bytes)?;

        match node {
            BtrfsNode::Internal(internal_node) => {
                for item in internal_node.items() {
                    Self::read_system_extent_tree_recurse(
                        devices,
                        item.block_number(),
                        extent_tree_items,
                    )?;
                }
            }

            BtrfsNode::Leaf(leaf_node) => {
                for item in leaf_node.items() {
                    extent_tree_items.insert(item.key(), item);
                }
            }
        }

        Ok(())
    }

    pub fn logical_to_physical_address(
        &self,
        logical_address: u64,
    ) -> Option<BtrfsPhysicalAddress> {
        // TODO waiting for range to land in stable rust

        for (ref chunk_item_offset, ref chunk_item) in self.chunk_items_by_offset.iter() {
            let chunk_item_offset = **chunk_item_offset;

            if logical_address >= chunk_item_offset
                && logical_address < (chunk_item_offset + chunk_item.data().chunk_size())
            {
                let chunk_item_stripe = chunk_item.stripes()[0];

                return Some(BtrfsPhysicalAddress::new(
                    chunk_item_stripe.device_id(),
                    logical_address - chunk_item_offset + chunk_item_stripe.offset(),
                ));
            }
        }

        None
    }
}
