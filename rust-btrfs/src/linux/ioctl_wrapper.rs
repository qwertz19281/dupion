use crate::linux::imports::*;

// ---------- btrfs

pub const BTRFS_IOCTL_MAGIC: u64 = 0x94;

ioctl_write_ptr! (
	ioctl_defrag_range, BTRFS_IOCTL_MAGIC, 16, IoctlDefragRangeArgs
);

ioctl_readwrite! (
	ioctl_dev_info, BTRFS_IOCTL_MAGIC, 30, IoctlDevInfoArgs
);

ioctl_readwrite! (
	ioctl_file_dedupe_range, BTRFS_IOCTL_MAGIC, 54, IoctlFileDedupeRange
);

ioctl_read! (
	ioctl_fs_info, BTRFS_IOCTL_MAGIC, 31, IoctlFsInfoArgs
);

ioctl_readwrite! (
	ioctl_space_info, BTRFS_IOCTL_MAGIC, 20, IoctlSpaceArgs
);

// ---------- other

ioctl_readwrite! (
	ioctl_fiemap,'f' as u64, 11, IoctlFiemap
);
