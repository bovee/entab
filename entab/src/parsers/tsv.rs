use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use memchr::memchr;

use crate::impl_reader;
use crate::parsers::common::NewLine;
use crate::parsers::extract_opt;
use crate::parsers::FromSlice;
use crate::record::{StateMetadata, Value};
use crate::EtError;

pub(crate) fn split<'a>(
    buffer: &mut Vec<&'a str>,
    line: &'a [u8],
    delim: u8,
    quote: u8,
) -> Result<usize, EtError> {
    let mut cur_pos = 0;
    let mut token_num = 0;
    while cur_pos < line.len() {
        if token_num >= buffer.len() {
            buffer.push("");
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
    Ok(token_num)
}

/// Track the current state of the TSV parser
#[derive(Clone, Debug, Default)]
pub struct TsvState {
    headers: Vec<String>,
    delim_char: u8,
    quote_char: u8,
}

impl<'b: 's, 'r, 's> FromSlice<'b, 's> for TsvState {
    // (delim_char, quote_char)
    type State = (u8, u8);

    fn parse(
        buffer: &[u8],
        eof: bool,
        consumed: &mut usize,
        _state: &mut Self::State,
    ) -> Result<bool, EtError> {
        NewLine::parse(buffer, eof, consumed, &mut 0)
    }

    fn get(
        &mut self,
        buffer: &'b [u8],
        (delim_char, quote_char): &'s Self::State,
    ) -> Result<(), EtError> {
        let con = &mut 0;
        let header = if let Some(NewLine(h)) = extract_opt::<NewLine>(buffer, false, con, &mut 0)? {
            h
        } else {
            return Err("could not read headers from TSV".into());
        };

        // prefill with something impossible so we can tell how big the header is
        let delim_str = [*delim_char];
        let mut buffer = vec![core::str::from_utf8(&delim_str)?; 32];
        let _ = split(&mut buffer, header, *delim_char, *quote_char)?;

        self.headers = buffer
            .into_iter()
            .filter(|i| i != &"\t")
            .map(String::from)
            .collect();
        self.delim_char = *delim_char;
        self.quote_char = *quote_char;
        Ok(())
    }
}

impl<'r> StateMetadata for TsvState {
    fn header(&self) -> Vec<&str> {
        let mut headers = Vec::new();
        for header in &self.headers {
            headers.push(header.as_ref());
        }
        headers
    }
}

/// Values from the current line of the TSV
#[derive(Debug, Default, PartialEq)]
pub struct TsvRecord<'r> {
    values: Vec<Value<'r>>,
}

impl<'b: 's, 's> FromSlice<'b, 's> for TsvRecord<'s> {
    type State = TsvState;

    fn parse(
        buffer: &[u8],
        eof: bool,
        consumed: &mut usize,
        _state: &mut Self::State,
    ) -> Result<bool, EtError> {
        NewLine::parse(buffer, eof, consumed, &mut 0)
    }

    fn get(&mut self, mut buffer: &'b [u8], state: &'s Self::State) -> Result<(), EtError> {
        if buffer.last() == Some(&b'\n') {
            buffer = &buffer[..buffer.len() - 1]
        }
        if buffer.last() == Some(&b'\r') {
            buffer = &buffer[..buffer.len() - 1]
        }
        let mut records = vec![""; state.headers.len()];
        let n_records = split(&mut records, buffer, state.delim_char, state.quote_char)?;
        if n_records != state.headers.len() {
            return Err("Line had a bad number of records".into());
        }
        self.values = records.into_iter().map(Value::from).collect();
        Ok(())
    }
}

impl<'r> From<TsvRecord<'r>> for Vec<Value<'r>> {
    fn from(record: TsvRecord<'r>) -> Self {
        record.values
    }
}

impl_reader!(TsvReader, TsvRecord, TsvRecord<'r>, TsvState, (u8, u8));

#[cfg(test)]
mod test {
    use super::*;

    use crate::readers::RecordReader;

    #[test]
    fn test_split() -> Result<(), EtError> {
        let mut buffer = Vec::new();

        assert_eq!(split(&mut buffer, b"1,2,3,4", b',', b'"')?, 4);
        assert_eq!(&buffer, &["1", "2", "3", "4"]);

        Ok(())
    }

    #[test]
    fn test_reader() -> Result<(), EtError> {
        const TEST_TEXT: &[u8] = b"header\nrow\nanother row";
        let mut pt = TsvReader::new(TEST_TEXT, Some((b'\t', b'"')))?;

        assert_eq!(&pt.headers(), &["header"]);
        let mut ix = 0;
        while let Some(TsvRecord { values }) = pt.next()? {
            match ix {
                0 => {
                    assert_eq!(values.len(), 1);
                    assert_eq!(values[0], "row".into());
                }
                1 => {
                    assert_eq!(values.len(), 1);
                    assert_eq!(values[0], "another row".into());
                }
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
        let mut pt = TsvReader::new(TEST_TEXT, Some((b'\t', b'"')))?;

        assert_eq!(&pt.headers(), &["header", "col1"]);
        let mut ix = 0;
        while let Some(TsvRecord { values }) = pt.next()? {
            match ix {
                0 => {
                    assert_eq!(values.len(), 2);
                    assert_eq!(values[0], "row".into());
                    assert_eq!(values[1], "2".into());
                }
                1 => {
                    assert_eq!(values.len(), 2);
                    assert_eq!(values[0], "another row".into());
                    assert_eq!(values[1], "3".into());
                }
                _ => return Err("bad line".into()),
            }
            ix += 1;
        }
        assert_eq!(ix, 2);
        Ok(())
    }

    // TODO: some failing tests
}
