use self::vfs::VfsId;

use super::*;
use state::State;
use opts::Opts;
use phase::Phase;
use parking_lot::RwLock;

pub mod platterwalker;
pub mod uringer;
pub mod common;
pub(crate) mod fiemap;

pub trait Driver {
    fn run(&mut self, state: &'static RwLock<State>, opts: &'static Opts, phase: Phase) -> AnyhowResult<()>;
    fn new(opts: &'static Opts) -> Self;
    fn read_phys(&mut self, entries: impl Iterator<Item=VfsId>, state: &'static RwLock<State>, opts: &'static Opts) -> AnyhowResult<()> {Ok(())}
}