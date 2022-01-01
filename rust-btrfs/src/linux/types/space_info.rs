use crate::linux::imports::*;

#[derive(Debug, Eq, PartialEq)]
pub struct SpaceInfo {
    pub group_type: GroupType,
    pub group_profile: GroupProfile,
    pub total_bytes: u64,
    pub used_bytes: u64,
}
