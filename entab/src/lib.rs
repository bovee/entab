#![cfg_attr(not(feature = "std"), no_std)]
//! entab is a library to parse different "record-formatted" file formats
//! into tabular form.
//!
//! An example reading a FASTA file and extracting all the ids:
//! ```
//! # #[cfg(feature = "std")] {
//! use std::fs::File;
//! use entab::Record;
//! use entab::buffer::ReadBuffer;
//! use entab::readers::RecordReader;
//! use entab::readers::fasta::FastaReader;
//!
//! let file = Box::new(File::open("./tests/data/sequence.fasta")?);
//! let buffer = ReadBuffer::new(file)?;
//! let mut reader = FastaReader::new(buffer, ())?;
//! while let Some(Record::Sequence { id, .. }) = reader.next()? {
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

pub use record::Record;
pub use utils::error::EtError;
