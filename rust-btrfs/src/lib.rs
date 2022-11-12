//! This library contains a partial implementation of a library for operating
//! with BTRFS filesystems. It is based on the C implementations of the BTRFS
//! utilities.
//!
//! It's home page is at [gitlab.wellbehavedsoftware.com]
//! (https://gitlab.wellbehavedsoftware.com/well-behaved-software/rust-btrfs).

#![allow(unused_parens)]
#![allow(clippy::identity_op)]
#![allow(clippy::module_inception)]
#![allow(clippy::missing_safety_doc)]
#![allow(clippy::needless_range_loop)]
#![allow(clippy::wrong_self_convention)]
#![allow(unaligned_references)]
#![deny(non_camel_case_types)]
#![deny(non_snake_case)]
#![deny(non_upper_case_globals)]
#![deny(unreachable_patterns)]
#![deny(unused_comparisons)]
#![deny(unused_must_use)]

#[macro_use]
extern crate lazy_static;

#[macro_use]
extern crate nix;

#[macro_use]
extern crate output;

extern crate byteorder;
extern crate chrono;
extern crate crc;
extern crate flate2;
extern crate itertools;
extern crate libc;
extern crate memmap;
extern crate minilzo;
extern crate uuid;

pub mod compress;
pub mod diskformat;
pub mod linux;

pub use crate::linux::*;
