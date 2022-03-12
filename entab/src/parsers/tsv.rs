use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::convert::TryInto;
use core::mem;

use memchr::memchr;

use crate::buffer::ReadBuffer;
use crate::parsers::common::NewLine;
use crate::parsers::extract_opt;
use crate::readers::RecordReader;
use crate::record::Value;
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
                }
                cur_pos += next;
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

/// A reader for TSV and other delimited tabular text formats
#[derive(Debug)]
pub struct TsvReader<'r> {
    rb: ReadBuffer<'r>,
    headers: Vec<String>,
    delim_char: u8,
    quote_char: u8,
    // by storing the vec in here, we save an allocation on each line
    cur_line: Vec<&'static str>,
}

impl<'r> TsvReader<'r> {
    /// Create a new `TsvReader`
    pub fn new<B>(data: B, params: (u8, u8)) -> Result<Self, EtError>
    where
        B: TryInto<ReadBuffer<'r>>,
        EtError: From<<B as TryInto<ReadBuffer<'r>>>::Error>,
    {
        let mut rb = data.try_into()?;
        let (delim_char, quote_char) = params;
        let con = &mut 0;
        let header = if let Some(NewLine(h)) = extract_opt::<NewLine>(rb.as_ref(), rb.eof, con, 0)?
        {
            h
        } else {
            return Err("could not read headers from TSV".into());
        };
        // prefill with something impossible so we can tell how big
        let mut buffer = vec!["\t"; 32];
        split(&mut buffer, header, delim_char, quote_char)?;
        let headers: Vec<String> = buffer
            .into_iter()
            .filter(|i| i != &"\t")
            .map(String::from)
            .collect();
        let n_headers = headers.len();

        rb.consumed += *con;
        Ok(TsvReader {
            rb,
            headers,
            cur_line: vec![""; n_headers],
            delim_char,
            quote_char,
        })
    }

    /// Return the next record from the TSV file
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Result<Option<&[&str]>, EtError> {
        let con = &mut 0;
        let buffer = &self.rb.as_ref()[self.rb.consumed..];
        let line = if let Some(NewLine(l)) = extract_opt::<NewLine>(buffer, self.rb.eof, con, 0)? {
            l
        } else {
            return Ok(None);
        };

        // this is nasty, but I *think* it's sound as long as no other
        // code messes with cur_line in between iterations of `next`?
        unsafe {
            split(
                mem::transmute(&mut self.cur_line),
                line,
                self.delim_char,
                self.quote_char,
            )
            .map_err(|e| e.add_context_from_readbuffer(&self.rb))?;
        }

        self.rb.consumed += *con;
        self.rb.record_pos += 1;
        // we pass along the headers too since they can be variable for tsvs
        Ok(Some(self.cur_line.as_ref()))
    }
}

impl<'r> RecordReader for TsvReader<'r> {
    fn next_record(&mut self) -> Result<Option<Vec<Value>>, EtError> {
        if let Some(record) = self.next()? {
            Ok(Some(record.iter().map(|i| (*i).into()).collect()))
        } else {
            Ok(None)
        }
    }

    fn headers(&self) -> Vec<String> {
        self.headers.clone()
    }

    fn metadata(&self) -> BTreeMap<String, Value> {
        BTreeMap::new()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_reader() -> Result<(), EtError> {
        const TEST_TEXT: &[u8] = b"header\nrow\nanother row";
        let mut pt = TsvReader::new(TEST_TEXT, (b'\t', b'"'))?;

        assert_eq!(&pt.headers(), &["header"]);
        let mut ix = 0;
        while let Some(fields) = pt.next()? {
            match ix {
                0 => assert_eq!(fields, &["row"]),
                1 => assert_eq!(fields, &["another row"]),
                _ => return Err("bad line".into()),
            }
            ix += 1;
        }
        assert_eq!(ix, 2);
        Ok(())
    }

    #[test]
    fn test_two_size_reader() -> Result<(), EtError> {
        const TEST_TEXT: &[u8] = b"header\tcol1\nrow\t2\nanother row\t3";
        let mut pt = TsvReader::new(TEST_TEXT, (b'\t', b'"'))?;

        assert_eq!(&pt.headers(), &["header", "col1"]);
        let mut ix = 0;
        while let Some(fields) = pt.next()? {
            match ix {
                0 => assert_eq!(fields, &["row", "2"]),
                1 => assert_eq!(fields, &["another row", "3"]),
                _ => return Err("bad line".into()),
            }
            ix += 1;
        }
        assert_eq!(ix, 2);
        Ok(())
    }

    // TODO: some failing tests
}
