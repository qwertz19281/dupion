use std::fmt::Error as FmtError;
use std::fmt::Formatter;
use std::fmt::{Debug, Display};

#[repr(C, packed)]
#[derive(Copy, Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BtrfsKey {
    object_id: u64,
    item_type: u8,
    offset: u64,
}

impl BtrfsKey {
    pub fn new(object_id: u64, item_type: u8, offset: u64) -> BtrfsKey {
        BtrfsKey {
            object_id,
            item_type,
            offset,
        }
    }

    pub fn object_id(&self) -> u64 {
        self.object_id
    }

    pub fn item_type(&self) -> u8 {
        self.item_type
    }

    pub fn offset(&self) -> u64 {
        self.offset
    }

    pub fn to_string_decimal(self) -> String {
        let Self {object_id, item_type, offset} = self;
        format!("{object_id}/{item_type} @ {offset}")
    }

    pub fn to_string_no_type(self) -> String {
        let Self {object_id, offset, ..} = self;
        format!("{object_id} @ 0x{offset:x}")
    }

    pub fn to_string_no_type_decimal(self) -> String {
        let Self {object_id, offset, ..} = self;
        format!("{object_id} @ {offset}")
    }
}

impl Debug for BtrfsKey {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), FmtError> {
        let Self {object_id, item_type, offset} = *self;
        write!(
            f,
            "BtrfsKey ({object_id}/{item_type} @ 0x{offset:x})"
        )
    }
}

impl Display for BtrfsKey {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let Self {object_id, item_type, offset} = *self;
        write!(
            f,
            "{object_id}/{item_type} @ 0x{offset:x}"
        )
    }
}
