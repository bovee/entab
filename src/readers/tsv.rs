use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::mem;
use std::io::Write;

use memchr::memchr;

use crate::buffer::ReadBuffer;
use crate::record::{BindT, ReaderBuilder, Record, RecordReader};
use crate::EtError;

fn split<'a>(
    buffer: &mut Vec<&'a str>,
    line: &'a [u8],
    delim: u8,
    quote: u8,
) -> Result<(), EtError> {
    let mut cur_pos = 0;
    let mut token_num = 0;
    while cur_pos < line.len() {
        if token_num >= buffer.len() {
            return Err("too many records".into());
        }
        let start_pos = cur_pos;
        if line[cur_pos] == quote {
            if let Some(next) = memchr(quote, &line[cur_pos + 1..]) {
                if line[next + 1] != delim {
                    return Err("quotes end mid-token".into());
                } else {
                    cur_pos += next;
                }
            } else {
                return Err("unclosed delimiter".into());
            }
        } else if let Some(next) = memchr(delim, &line[cur_pos..]) {
            cur_pos += next;
        } else {
            cur_pos = line.len();
        };
        buffer[token_num] = alloc::str::from_utf8(&line[start_pos..cur_pos])?;
        cur_pos += 1;
        token_num += 1;
    }
    // TODO: check that token_num matches the length of the buffer
    // (unless these are the headers; in which case we don't care
    // because we'll trim elsewhere)
    Ok(())
}

impl Record for &[&str] {
    fn size(&self) -> usize {
        <[&str]>::len(self)
    }

    fn write_field(&self, num: usize, writer: &mut dyn Write) -> Result<(), EtError> {
        writer.write_all(self[num].as_bytes())?;
        Ok(())
    }
}

pub struct TsvRecordT;
impl<'b> BindT<'b> for TsvRecordT {
    type Assoc = &'b [&'b str];
}

pub struct TsvReaderBuilder {
    delim_char: u8,
    quote_char: u8,
}

impl TsvReaderBuilder {
    pub fn delim(mut self, delim_char: u8) -> Self {
        self.delim_char = delim_char;
        self
    }

    pub fn quote(mut self, quote_char: u8) -> Self {
        self.quote_char = quote_char;
        self
    }
}

impl Default for TsvReaderBuilder {
    fn default() -> Self {
        TsvReaderBuilder {
            delim_char: b'\t',
            quote_char: b'"',
        }
    }
}

impl ReaderBuilder for TsvReaderBuilder {
    type Item = TsvRecordT;

    fn to_reader<'r>(
        &self,
        mut rb: ReadBuffer<'r>,
    ) -> Result<Box<dyn RecordReader<Item = Self::Item> + 'r>, EtError> {
        let headers: Vec<String> = if let Some(line) = rb.read_line()? {
            // prefill with something impossible so we can tell how big
            let mut buffer = vec!["\t"; 32];
            split(&mut buffer, line, self.delim_char, self.quote_char)?;
            buffer
                .into_iter()
                .filter(|i| i != &"\t")
                .map(String::from)
                .collect()
        } else {
            return Err(EtError::new("could not read headers from TSV").fill_pos(&rb));
        };
        let n_headers = headers.len();

        let reader = TsvReader {
            rb,
            headers,
            cur_line: vec![""; n_headers],
            delim_char: self.delim_char,
            quote_char: self.quote_char,
        };

        Ok(Box::new(reader))
    }
}

pub struct TsvReader<'r> {
    rb: ReadBuffer<'r>,
    headers: Vec<String>,
    delim_char: u8,
    quote_char: u8,
    cur_line: Vec<&'r str>,
}

impl<'r> RecordReader for TsvReader<'r> {
    type Item = TsvRecordT;

    fn headers(&self) -> Vec<&str> {
        self.headers.iter().map(|i| &**i).collect()
    }

    fn next(&mut self) -> Result<Option<&[&str]>, EtError> {
        if let Some(line) = self.rb.read_line()? {
            // this is nasty, but I *think* it's sound as long as no other
            // code messes with cur_line in between iterations of `next`?
            //
            unsafe {
                split(
                    mem::transmute(&mut self.cur_line),
                    line,
                    self.delim_char,
                    self.quote_char,
                )
                .map_err(|e| e.fill_pos(&self.rb))?;
            }
        } else {
            return Ok(None);
        }
        Ok(Some(&self.cur_line))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::buffer::ReadBuffer;
    use std::io::Cursor;

    #[test]
    fn test_reader() -> Result<(), EtError> {
        const TEST_TEXT: &str = "header\nrow\nanother row";
        let rb = ReadBuffer::with_capacity(5, Box::new(Cursor::new(TEST_TEXT)))?;
        let mut pt = TsvReaderBuilder::default().to_reader(rb)?;
        assert_eq!(pt.headers(), vec!["header"]);

        let mut ix = 0;
        while let Some(l) = pt.next()? {
            match ix {
                0 => assert_eq!(l, &["row"]),
                1 => assert_eq!(l, &["another row"]),
                _ => return Err("bad line".into()),
            }
            ix += 1;
        }
        assert_eq!(ix, 2);
        Ok(())
    }

    #[test]
    fn test_two_size_reader() -> Result<(), EtError> {
        const TEST_TEXT: &str = "header\tcol1\nrow\t2\nanother row\t3";
        let rb = ReadBuffer::with_capacity(5, Box::new(Cursor::new(TEST_TEXT)))?;
        let mut pt = TsvReaderBuilder::default().to_reader(rb)?;
        assert_eq!(pt.headers(), vec!["header", "col1"]);

        let mut ix = 0;
        while let Some(l) = pt.next()? {
            match ix {
                0 => assert_eq!(l, &["row", "2"]),
                1 => assert_eq!(l, &["another row", "3"]),
                _ => return Err("bad line".into()),
            }
            ix += 1;
        }
        assert_eq!(ix, 2);
        Ok(())
    }

    // TODO: some failing tests
}
