use crate::diskformat::*;

#[derive(Clone, Debug)]
pub struct BtrfsInvalidItem<'a> {
    header: &'a BtrfsLeafItemHeader,
    data_bytes: &'a [u8],
    error: String,
}

impl<'a> BtrfsInvalidItem<'a> {
    pub fn new(
        header: &'a BtrfsLeafItemHeader,
        data_bytes: &'a [u8],
        error: String,
    ) -> BtrfsInvalidItem<'a> {
        BtrfsInvalidItem {
            header,
            data_bytes,
            error,
        }
    }
}

impl<'a> BtrfsLeafItemContents<'a> for BtrfsInvalidItem<'a> {
    fn header(&self) -> &BtrfsLeafItemHeader {
        self.header
    }
}
