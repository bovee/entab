use std::error::Error;
use std::fmt;
use std::io::Error as IoError;
use std::str::Utf8Error;
use std::string::FromUtf8Error;

pub mod buffer;
pub mod compression;
pub mod filetype;
pub mod readers;
pub mod record;

#[derive(Debug)]
pub struct EtError {
    msg: String,
    byte: Option<u64>,
    record: Option<u64>,
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
            orig_err: None,
        }
    }

    pub fn fill_pos(mut self, reader: &buffer::ReadBuffer) -> Self {
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
            orig_err: None,
        }
    }
}

impl From<FromUtf8Error> for EtError {
    fn from(error: FromUtf8Error) -> Self {
        EtError {
            msg: error.description().to_string(),
            byte: None,
            record: None,
            orig_err: Some(Box::new(error)),
        }
    }
}

impl From<IoError> for EtError {
    fn from(error: IoError) -> Self {
        EtError {
            msg: error.description().to_string(),
            byte: None,
            record: None,
            orig_err: Some(Box::new(error)),
        }
    }
}

impl From<Utf8Error> for EtError {
    fn from(error: Utf8Error) -> Self {
        EtError {
            msg: error.description().to_string(),
            byte: None,
            record: None,
            orig_err: Some(Box::new(error)),
        }
    }
}
