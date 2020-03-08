use std::error::Error;
use std::fmt;
use std::io::Error as IoError;
use std::str::Utf8Error;
use std::string::FromUtf8Error;

pub mod buffer;
pub mod compression;
pub mod mime;
pub mod record;


pub const BUFFER_SIZE: usize = 1000;

#[derive(Debug)]
struct EtError {
    msg: String,
    line: Option<u64>,
    orig_err: Option<Box<dyn Error>>,
}

impl fmt::Display for EtError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.msg)
    }
}

impl Error for EtError {
    fn description(&self) -> &str {
        &self.msg
    }

    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.orig_err.as_ref().map(|c| &**c as &(dyn Error + 'static))
    }
}

impl From<FromUtf8Error> for EtError {
    fn from(error: FromUtf8Error) -> Self {
        EtError {
            msg: error.description().to_string(),
            line: None,
            orig_err: Some(Box::new(error)),
        }
    }
}

impl From<IoError> for EtError {
    fn from(error: IoError) -> Self {
        EtError {
            msg: error.description().to_string(),
            line: None,
            orig_err: Some(Box::new(error)),
        }
    }
}

impl From<Utf8Error> for EtError {
    fn from(error: Utf8Error) -> Self {
        EtError {
            msg: error.description().to_string(),
            line: None,
            orig_err: Some(Box::new(error)),
        }
    }
}
