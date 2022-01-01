use crate::linux::imports::*;

#[derive(Debug, Eq, PartialEq)]
pub struct DeviceInfo {
    pub device_id: u64,
    pub uuid: Uuid,
    pub bytes_used: u64,
    pub total_bytes: u64,
    pub path: OsString,
}
