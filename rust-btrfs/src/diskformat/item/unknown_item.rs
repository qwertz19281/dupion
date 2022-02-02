use crate::diskformat::*;

#[derive(Clone, Debug)]
pub struct BtrfsUnknownItem<'a> {
    header: &'a BtrfsLeafItemHeader,
    data_bytes: &'a [u8],
}

impl<'a> BtrfsUnknownItem<'a> {
    pub fn new(header: &'a BtrfsLeafItemHeader, data_bytes: &'a [u8]) -> BtrfsUnknownItem<'a> {
        BtrfsUnknownItem { header, data_bytes }
    }
}

impl<'a> BtrfsLeafItemContents<'a> for BtrfsUnknownItem<'a> {
    fn header(&self) -> &BtrfsLeafItemHeader {
        self.header
    }
}
