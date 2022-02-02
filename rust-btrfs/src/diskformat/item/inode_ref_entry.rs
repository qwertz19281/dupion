use std::borrow::Cow;
use std::fmt::Debug;
use std::fmt::Error as FmtError;
use std::fmt::Formatter;
use std::mem;

use crate::diskformat::*;

#[derive(Copy, Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BtrfsInodeRefEntry<'a> {
    header: &'a BtrfsLeafItemHeader,
    data_bytes: &'a [u8],
}

impl<'a> BtrfsInodeRefEntry<'a> {
    pub fn from_bytes(
        header: &'a BtrfsLeafItemHeader,
        data_bytes: &'a [u8],
    ) -> Result<BtrfsInodeRefEntry<'a>, String> {
        // sanity check

        if data_bytes.len() < mem::size_of::<BtrfsInodeRefData>() {
            return Err(format!(
                "Must be at least 0x{:x} bytes",
                mem::size_of::<BtrfsInodeRefData>()
            ));
        }

        // create inode ref

        let inode_ref = BtrfsInodeRefEntry { header, data_bytes };

        // sanity check

        if data_bytes.len()
            != (mem::size_of::<BtrfsInodeRefData>() + inode_ref.name_length() as usize)
        {
            return Err(format!(
                "Must be at exactly 0x{:x} bytes",
                mem::size_of::<BtrfsInodeRefData>() + inode_ref.name_length() as usize
            ));
        }

        // return

        Ok(inode_ref)
    }

    pub fn header(&self) -> &BtrfsLeafItemHeader {
        self.header
    }

    pub fn data(&self) -> &BtrfsInodeRefData {
        unsafe { &*(self.data_bytes.as_ptr() as *const BtrfsInodeRefData) }
    }

    pub fn sequence(&self) -> u64 {
        self.data().sequence
    }

    pub fn name_length(&self) -> u16 {
        self.data().name_length
    }

    pub fn name(&'a self) -> &'a [u8] {
        &self.data_bytes[mem::size_of::<BtrfsInodeRefData>()
            ..mem::size_of::<BtrfsInodeRefData>() + self.name_length() as usize]
    }

    pub fn name_to_string_lossy(&self) -> Cow<str> {
        String::from_utf8_lossy(self.name())
    }
}

impl<'a> BtrfsLeafItemContents<'a> for BtrfsInodeRefEntry<'a> {
    fn header(&self) -> &BtrfsLeafItemHeader {
        self.header
    }
}

impl<'a> Debug for BtrfsInodeRefEntry<'a> {
    fn fmt(&self, formatter: &mut Formatter) -> Result<(), FmtError> {
        let mut debug_struct = formatter.debug_struct("BtrfsInodeRefEntry");

        debug_struct.field(
            "key",
            &NakedString::from(self.key().to_string_no_type_decimal()),
        );

        debug_struct.field("sequence", &self.sequence());

        debug_struct.field("name", &self.name_to_string_lossy());

        debug_struct.finish()
    }
}
