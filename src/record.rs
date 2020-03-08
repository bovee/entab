use std::collections::HashMap;
use std::io::{Error, BufRead};

use memchr::memchr;

use crate::EtError;
use crate::buffer::ReadBuffer;


pub struct PassThrough<'a> {
    rb: ReadBuffer<'a>,
    endline: usize,
}

impl<'a> PassThrough<'a> {
    fn advance(&mut self) -> Result<(), EtError> {
        // TODO: don't do this unless there's no newline b/c unncecessary copying
        let mut buf = self.rb.fill_buf()?;

        let endline = loop {
            if let Some(e) = memchr(b'\n', &buf) {
                break e;
            }
            buf = self.rb.fill_buf()?;
        };
        buf.consume(endline);
        self.endline = endline;
        Ok(())
    }
    
    fn get(&self) -> &[u8] {
        &self.rb.as_ref()[..self.endline]
    }
}

trait Record {
    fn as_line(&self) -> &str;
}

impl<'a> RecordReader<'a> for PassThrough<'a> {
    type Item = [&'a str];

    fn metadata(&mut self) -> HashMap<String, String> {
        HashMap::new()
    }

    fn header(&mut self) -> Result<Vec<String>, EtError> {
        self.advance()?;
        let l = self.get();
        Ok(vec![String::from_utf8(l.to_vec())?])
    }

    fn next(&'a mut self) -> Result<Option<&Self::Item>, EtError> {
        self.advance()?;
        let l = self.get();
        Ok(Some(&[std::str::from_utf8(l)?]))
    }
}

pub trait RecordReader<'s> {
    type Item: ?Sized + 's;

    fn metadata(&mut self) -> HashMap<String, String>;
    fn header(&mut self) -> Result<Vec<String>, EtError>;
    fn next(&'s mut self) -> Result<Option<&Self::Item>, EtError>;
    // fn write_tsv(&mut self, writer: &mut dyn Write) -> Result<(), Error>;
    // fn to_json(&self)
}
