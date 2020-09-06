#![cfg_attr(not(feature = "std"), no_std)]
#![deny(
    missing_debug_implementations,
    missing_copy_implementations,
    trivial_casts,
    trivial_numeric_casts,
    unreachable_pub,
    unused_import_braces,
    unused_qualifications,
    unused_results
)]
//! entab is a library to parse different "record-formatted" file formats
//! into tabular form.
//!
//! An example reading a FASTA file and extracting all the ids:
//! ```
//! # #[cfg(feature = "std")] {
//! use std::fs::File;
//! use entab::buffer::ReadBuffer;
//! use entab::readers::fasta::{FastaReader, FastaRecord};
//!
//! let file = Box::new(File::open("./tests/data/sequence.fasta")?);
//! let buffer = ReadBuffer::new(file)?;
//! let mut reader = FastaReader::new(buffer, ())?;
//! while let Some(FastaRecord { id, .. }) = reader.next()? {
//!     println!("{}", id);
//! }
//! # }
//! # use entab::EtError;
//! # Ok::<(), EtError>(())
//! ```

extern crate alloc;

/// The buffer interface that underlies the file readers
pub mod buffer;
/// Generic file decompression
#[cfg(feature = "std")]
pub mod compression;
/// File format inference
pub mod filetype;
/// Lightweight parsers to read records out of buffers
mod parsers;
/// Parsers for specific file formats
pub mod readers;
/// Record and abstract record reading
pub mod record;
/// Miscellanous utility functions and error handling
pub mod utils;

pub use utils::error::EtError;
