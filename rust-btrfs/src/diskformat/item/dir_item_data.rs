use super::super::prelude::*;

#[repr(C, packed)]
#[derive(Copy, Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BtrfsDirItemData {
    pub child_key: BtrfsKey,
    pub transaction_id: u64,
    pub data_size: u16,
    pub name_size: u16,
    pub child_type: u8,
}

impl Debug for BtrfsDirItemData {
    fn fmt(&self, formatter: &mut Formatter) -> Result<(), FmtError> {
        let Self {
            child_key,
            transaction_id,
            data_size,
            name_size,
            child_type,
        } = *self;

        let mut debug_struct = formatter.debug_struct("BtrfsDirItemData");

        debug_struct.field("child_key", &NakedString::from(child_key.to_string()));

        debug_struct.field("transaction_id", &transaction_id);

        debug_struct.field("data_size", &data_size);

        debug_struct.field("name_size", &name_size);

        debug_struct.field("child_type", &child_type);

        debug_struct.finish()
    }
}
