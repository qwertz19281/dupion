use std::fs::File;
use std::io::Seek;
use std::io::SeekFrom;
use std::path::PathBuf;

use memmap::Mmap;
use memmap::Protection;

use super::super::*;

pub struct BtrfsMmapDeviceSet {
    mmaps: Vec<Mmap>,
}

impl BtrfsMmapDeviceSet {
    pub fn open(device_paths: &[PathBuf]) -> Result<BtrfsMmapDeviceSet, String> {
        // open devices

        let mut mmaps: Vec<Mmap> = Vec::new();

        for device_path in device_paths.iter() {
            let mut file = File::open(device_path).map_err(|error| {
                format!("Error opening {}: {}", device_path.to_string_lossy(), error)
            })?;

            let file_size = file.seek(SeekFrom::End(0)).map_err(|error| {
                format!(
                    "Error finding file size for {}: {}",
                    device_path.to_string_lossy(),
                    error
                )
            })?;

            let mmap = Mmap::open_with_offset(&file, Protection::Read, 0, file_size as usize)
                .map_err(|error| {
                    format!("Error mmaping {}: {}", device_path.to_string_lossy(), error)
                })?;

            mmaps.push(mmap);
        }

        Ok(BtrfsMmapDeviceSet { mmaps })
    }

    pub fn devices(&self) -> Result<BtrfsDeviceSet<'_>, String> {
        BtrfsDeviceSet::new(
            &self
                .mmaps
                .iter()
                .map(|mmap| unsafe { mmap.as_slice() })
                .collect::<Vec<&[u8]>>(),
        )
    }
}
