use parking_lot::RwLock;

use super::opts::Opts;
use super::platterwalker::PlatterWalker;
use super::state::State;
use super::vfs::VfsId;
use super::Driver;
#[cfg(feature = "io_uring")]
use super::uringer::Uringer;

pub enum AutoDriver {
    #[cfg(feature = "io_uring")]
    Uringer(Uringer),
    Legacy(PlatterWalker),
}

impl Driver for AutoDriver {
    #[cfg(feature = "io_uring")]
    fn new(o: &mut Opts) -> Self {
        if o.read_archives {
            dprintln!("Disabling uring mode in archive mode");
            o.uring = false;
        }
        if o.uring {
            match Uringer::try_new(o) {
                Ok(v) => Self::Uringer(v),
                Err(e) => {
                    dprintln!("Failed to initialize io_uring mode, fallback to legacy mode:");
                    eprintln!("{e}");
                    o.uring = false;
                    Self::Legacy(PlatterWalker::new(o))
                },
            }
        } else {
            Self::Legacy(PlatterWalker::new(o))
        }
    }

    #[cfg(not(feature = "io_uring"))]
    fn new(o: &mut Opts) -> Self {
        if o.uring {
            //dprintln!("This build isn't compiled with io_uring support, fallback to legacy mode.")
        }
        o.uring = false;
        Self::Legacy(PlatterWalker::new(o))
    }
    
    fn run(&mut self, state: &'static RwLock<State>, opts: &'static Opts, phase: super::phase::Phase) -> anyhow::Result<()> {
        match self {
            #[cfg(feature = "io_uring")]
            AutoDriver::Uringer(v) => v.run(state, opts, phase),
            AutoDriver::Legacy(v) => v.run(state, opts, phase),
        }
    }
    
    fn read_phys(&mut self, entries: impl Iterator<Item=VfsId>, state: &'static RwLock<State>, opts: &'static Opts) -> anyhow::Result<()> {
        match self {
            #[cfg(feature = "io_uring")]
            AutoDriver::Uringer(v) => v.read_phys(entries, state, opts),
            AutoDriver::Legacy(v) => v.read_phys(entries, state, opts),
        }
    }
}
