use crate::linux::imports::*;

#[derive(Debug, Eq, PartialEq)]
pub struct DedupeRange {
    pub src_offset: u64,
    pub src_length: u64,
    pub dest_infos: Vec<DedupeRangeDestInfo>,
}
