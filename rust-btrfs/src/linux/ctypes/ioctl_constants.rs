pub const UUID_SIZE: usize = 16;
pub const DEVICE_PATH_NAME_MAX: usize = 1024;

pub const AVAIL_ALLOC_BIT_SINGLE: u64 = 1 << 48;

pub const BLOCK_GROUP_DATA: u64 = 1 << 0;
pub const BLOCK_GROUP_SYSTEM: u64 = 1 << 1;
pub const BLOCK_GROUP_METADATA: u64 = 1 << 2;

pub const BLOCK_GROUP_RAID0: u64 = 1 << 3;
pub const BLOCK_GROUP_RAID1: u64 = 1 << 4;
pub const BLOCK_GROUP_DUP: u64 = 1 << 5;
pub const BLOCK_GROUP_RAID10: u64 = 1 << 6;
pub const BLOCK_GROUP_RAID5: u64 = 1 << 7;
pub const BLOCK_GROUP_RAID6: u64 = 1 << 8;

pub const BLOCK_GROUP_RESERVED: u64 = AVAIL_ALLOC_BIT_SINGLE;

pub const BLOCK_GROUP_DATA_AND_METADATA: u64 = (BLOCK_GROUP_DATA | BLOCK_GROUP_METADATA);

pub const BLOCK_GROUP_TYPE_MASK: u64 =
    (BLOCK_GROUP_DATA | BLOCK_GROUP_SYSTEM | BLOCK_GROUP_METADATA);

pub const BLOCK_GROUP_TYPE_AND_RESERVED_MASK: u64 = (BLOCK_GROUP_TYPE_MASK | BLOCK_GROUP_RESERVED);

pub const BLOCK_GROUP_PROFILE_MASK: u64 = (BLOCK_GROUP_RAID0
    | BLOCK_GROUP_RAID1
    | BLOCK_GROUP_RAID5
    | BLOCK_GROUP_RAID6
    | BLOCK_GROUP_DUP
    | BLOCK_GROUP_RAID10);

pub const COMPRESS_NONE: u32 = 0;
pub const COMPRESS_ZLIB: u32 = 1;
pub const COMPRESS_LZO: u32 = 2;

pub const DEFRAG_RANGE_COMPRESS: u64 = 1;
pub const DEFRAG_RANGE_START_IO: u64 = 2;

pub const FILE_DEDUPE_RANGE_SAME: i32 = 0;
pub const FILE_DEDUPE_RANGE_DIFFERS: i32 = 1;
