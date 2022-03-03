#![cfg_attr(not(feature = "std"), no_std)]
#![deny(
    missing_debug_implementations,
    missing_docs,
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
//! Entab provides two different ways to parse each file it supports. If you
//! know the type of the file you'll be reading, you generally want to use the
//! specific parser for that file type which will return a record of a specific
//! type. For example, to parse the IDs out of a FASTA file you might do the
//! following:
//! ```
//! # #[cfg(feature = "std")] {
//! use std::fs::File;
//! use entab::readers::fasta::{FastaReader, FastaRecord};
//!
//! let file = File::open("./tests/data/sequence.fasta")?;
//! let mut reader = FastaReader::new(file, ())?;
//! while let Some(FastaRecord { id, .. }) = reader.next()? {
//!     println!("{}", id);
//! }
//! # }
//! # use entab::EtError;
//! # Ok::<(), EtError>(())
//! ```
//!
//! Alternatively, you may not know the type of file when writing your code so
//! you may want to abstract over as many types as possible. This is where the
//! slower, generic parser framework is used (for example, in the bindings
//! libraries also). This framework takes a parser name which can be
//! autodetected via `entab::filetype::sniff_reader_filetype` and
//! `entab::filetype::FileType::to_parser_name`:
//! ```
//! # #[cfg(feature = "std")] {
//! use std::fs::File;
//! use entab::readers::get_reader;
//!
//! let file = File::open("./tests/data/sequence.fasta")?;
//! let mut reader = get_reader("fasta", file)?;
//! while let Some(record) = reader.next_record()? {
//!     println!("{:?}", record[0]);
//! }
//! # }
//! # use entab::EtError;
//! # Ok::<(), EtError>(())
//! ```

extern crate alloc;

/// The buffer interface that underlies the file readers
pub mod buffer;
/// Part of a buffer used to parse records across threads
pub mod chunk;
/// Generic file decompression
#[cfg(feature = "std")]
pub mod compression;
/// Miscellanous utility functions and error handling
pub mod error;
/// File format inference
pub mod filetype;
/// Lightweight parsers to read records out of buffers
mod parsers;
/// Parsers for specific file formats
pub mod readers;
/// Record and abstract record reading
pub mod record;

pub use error::EtError;
