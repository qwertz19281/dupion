use super::*;
use std::sync::RwLock;
use state::State;
use opts::Opts;
use phase::Phase;

pub mod platterwalker;

pub trait Driver {
    fn run(&mut self, state: &'static RwLock<State>, opts: &'static Opts, phase: Phase) -> AnyhowResult<()>;
    fn new() -> Self;
}