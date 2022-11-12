use crate::linux::imports::*;

#[derive(Debug, Eq, PartialEq)]
pub enum CompressionType {
    None,
    Zlib,
    Lzo,
}

impl From<CompressionType> for u32 {
    fn from(c: CompressionType) -> Self {
        use crate::CompressionType::*;

        match c {
            None => COMPRESS_NONE,
            Zlib => COMPRESS_ZLIB,
            Lzo => COMPRESS_LZO,
        }
    }
}
