#![cfg_attr(not(feature = "std"), no_std)]
#![allow(clippy::module_name_repetitions)]
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
//! use entab::parsers::fasta::{FastaReader, FastaRecord};
//!
//! let file = File::open("./tests/data/sequence.fasta")?;
//! let mut reader = FastaReader::new(file, None)?;
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
//! for different languages). This framework can optionally take a `parser_name`
//! to force it to use that specific parser and optional params to control
//! parser options.
//! ```
//! # #[cfg(feature = "std")] {
//! use std::fs::File;
//! use entab::filetype::FileType;
//! use entab::readers::get_reader;
//!
//! let file = File::open("./tests/data/sequence.fasta")?;
//! let (mut reader, _) = get_reader(file, None, None)?;
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
/// Generic file decompression
pub mod compression;
/// Miscellanous utility functions and error handling
pub mod error;
/// File format inference
pub mod filetype;
/// Lightweight parsers to read records out of buffers
pub mod parsers;
/// Parsers for specific file formats
pub mod readers;
/// Record and abstract record reading
pub mod record;

pub use error::EtError;
