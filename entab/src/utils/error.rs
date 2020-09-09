#[cfg(feature = "std")]
use alloc::boxed::Box;
use alloc::str::Utf8Error;
use alloc::string::{FromUtf8Error, String, ToString};
use core::fmt;
use core::num::ParseIntError;
#[cfg(feature = "std")]
use std::error::Error;
#[cfg(feature = "std")]
use std::io::Error as IoError;

use crate::buffer::ReadBuffer;

#[derive(Debug)]
/// The Error struct for entab
pub struct EtError {
    /// A succinct message describing the error
    pub msg: String,
    /// At what byte in a the file (if any), the error occured
    pub byte: Option<u64>,
    /// At what record in a the file (if any), the error occured.
    ///
    /// Note, this may not be the same as the index of the iterator
    /// if the underlying file type groups e.g. record information by
    /// time slice.
    pub record: Option<u64>,
    #[cfg(feature = "std")]
    orig_err: Option<Box<dyn Error>>,
}

impl EtError {
    /// Create a new EtError with a display message of `msg`
    pub fn new<T>(msg: T) -> Self
    where
        T: Into<String>,
    {
        EtError {
            msg: msg.into(),
            byte: None,
            record: None,
            #[cfg(feature = "std")]
            orig_err: None,
        }
    }

    /// Fill the positional error information from a ReadBuffer
    ///
    /// Used to display e.g. where a parsing error in a file occured.
    pub fn fill_pos(mut self, reader: &ReadBuffer) -> Self {
        self.record = Some(reader.record_pos);
        self.byte = Some(reader.get_byte_pos());
        self
    }
}

impl fmt::Display for EtError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.msg)
    }
}

#[cfg(feature = "std")]
impl Error for EtError {
    fn description(&self) -> &str {
        &self.msg
    }

    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.orig_err.as_ref().map(|c| {
            let b: &(dyn Error + 'static) = &**c;
            b
        })
    }
}

impl From<&str> for EtError {
    fn from(error: &str) -> Self {
        EtError {
            msg: error.to_string(),
            byte: None,
            record: None,
            #[cfg(feature = "std")]
            orig_err: None,
        }
    }
}

impl From<String> for EtError {
    fn from(msg: String) -> Self {
        EtError {
            msg,
            byte: None,
            record: None,
            #[cfg(feature = "std")]
            orig_err: None,
        }
    }
}

impl From<FromUtf8Error> for EtError {
    fn from(error: FromUtf8Error) -> Self {
        EtError {
            msg: error.to_string(),
            byte: None,
            record: None,
            #[cfg(feature = "std")]
            orig_err: Some(Box::new(error)),
        }
    }
}

#[cfg(feature = "std")]
impl From<IoError> for EtError {
    fn from(error: IoError) -> Self {
        EtError {
            msg: error.to_string(),
            byte: None,
            record: None,
            #[cfg(feature = "std")]
            orig_err: Some(Box::new(error)),
        }
    }
}

impl From<Utf8Error> for EtError {
    fn from(error: Utf8Error) -> Self {
        EtError {
            msg: error.to_string(),
            byte: None,
            record: None,
            #[cfg(feature = "std")]
            orig_err: Some(Box::new(error)),
        }
    }
}

impl From<ParseIntError> for EtError {
    fn from(error: ParseIntError) -> Self {
        EtError {
            msg: error.to_string(),
            byte: None,
            record: None,
            #[cfg(feature = "std")]
            orig_err: Some(Box::new(error)),
        }
    }
}
