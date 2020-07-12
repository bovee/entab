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
pub struct EtError {
    pub msg: String,
    pub byte: Option<u64>,
    pub record: Option<u64>,
    #[cfg(feature = "std")]
    orig_err: Option<Box<dyn Error>>,
}

impl EtError {
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

    pub fn fill_pos(mut self, reader: &ReadBuffer) -> Self {
        let (record_pos, byte_pos) = reader.get_pos();
        self.record = Some(record_pos);
        self.byte = Some(byte_pos);
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
        self.orig_err
            .as_ref()
            .map(|c| &**c as &(dyn Error + 'static))
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
            #[cfg(not(feature = "std"))]
            msg: error.to_string(),
            #[cfg(feature = "std")]
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
            #[cfg(not(feature = "std"))]
            msg: error.to_string(),
            #[cfg(feature = "std")]
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
            #[cfg(not(feature = "std"))]
            msg: error.to_string(),
            #[cfg(feature = "std")]
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
            #[cfg(not(feature = "std"))]
            msg: error.to_string(),
            #[cfg(feature = "std")]
            msg: error.to_string(),
            byte: None,
            record: None,
            #[cfg(feature = "std")]
            orig_err: Some(Box::new(error)),
        }
    }
}
