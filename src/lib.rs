#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod buffer;
#[cfg(feature = "std")]
pub mod compression;
pub mod filetype;
pub mod readers;
pub mod record;
pub mod utils;

pub use utils::error::EtError;
