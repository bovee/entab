use alloc::borrow::Cow;
use alloc::str::from_utf8;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use crate::impl_reader;
use crate::parsers::common::NewLine;
use crate::parsers::extract_opt;
use crate::parsers::tsv_inference::{
    sniff_params_from_data, sniff_types_from_data, split, TsvFieldType, DEFAULT_DELIM,
    DEFAULT_QUOTE,
};
use crate::parsers::FromSlice;
use crate::record::{StateMetadata, Value};
use crate::EtError;

/// Parameters for parsing TSVs
///
/// For some documents about possible variations in the TSV "format" see:
/// RFC: https://datatracker.ietf.org/doc/html/rfc4180
/// Frictionless data spec: https://specs.frictionlessdata.io//csv-dialect/
#[derive(Clone, Debug)]
pub struct TsvParams {
    /// The character used to separate fields.
    pub delim_char: Option<u8>,
    /// The character used to quote text fields or fields containing the delimiter.
    pub quote_char: Option<u8>,
    /// The number of lines to skip before the column titles and data start.
    pub skip_lines: Option<usize>,
    /// Automatically determine the delimiter, quoting character, and number of lines to skip.
    pub sniff_file: bool,
    /// Automatically determine the types of each of the fields in the TSV.
    pub infer_types: bool,
    /// The data types of each of the fields in the TSV
    pub types: Vec<TsvFieldType>,
}

impl Default for TsvParams {
    fn default() -> Self {
        TsvParams {
            delim_char: None,
            quote_char: None,
            skip_lines: None,
            sniff_file: true,
            infer_types: true,
            types: vec![],
        }
    }
}

impl TsvParams {
    /// Set the delimiter character
    pub fn delim(mut self, c: u8) -> Self {
        self.delim_char = Some(c);
        self
    }

    /// Set the character used for quoting delimiters
    pub fn quote(mut self, c: u8) -> Self {
        self.quote_char = Some(c);
        self
    }
}

/// Track the current state of the TSV parser
#[derive(Clone, Debug, Default)]
pub struct TsvState {
    headers: Vec<String>,
    types: Option<Vec<TsvFieldType>>,
    delim_char: u8,
    quote_char: u8,
}

impl<'b: 's, 'r, 's> FromSlice<'b, 's> for TsvState {
    type State = TsvParams;

    fn parse(
        buffer: &[u8],
        eof: bool,
        consumed: &mut usize,
        state: &mut Self::State,
    ) -> Result<bool, EtError> {
        if state.sniff_file {
            sniff_params_from_data(state, buffer);
        }
        if state.infer_types {
            sniff_types_from_data(state, buffer);
        }
        let con = &mut 0;
        for _ in 0..state.skip_lines.unwrap_or(0) {
            if extract_opt::<NewLine>(buffer, false, con, &mut 0)?.is_none() {
                return Err("could not skip header lines".into());
            }
        }
        if !NewLine::parse(&buffer[*con..], eof, con, &mut 0)? {
            return Ok(false);
        }
        *consumed += *con;
        Ok(true)
    }

    fn get(&mut self, buffer: &'b [u8], state: &'s Self::State) -> Result<(), EtError> {
        let con = &mut 0;
        for _ in 0..state.skip_lines.unwrap_or(0) {
            if extract_opt::<NewLine>(buffer, false, con, &mut 0)?.is_none() {
                return Err("could not skip header lines".into());
            }
        }
        let header = if let Some(NewLine(h)) = extract_opt::<NewLine>(buffer, false, con, &mut 0)? {
            h
        } else {
            return Err("could not read headers from TSV".into());
        };
        if state.infer_types {
            self.types = Some(state.types.clone());
        }

        self.delim_char = state.delim_char.unwrap_or(DEFAULT_DELIM);
        self.quote_char = state.quote_char.unwrap_or(DEFAULT_QUOTE);

        // prefill with something impossible so we can tell how big the header is
        let delim_slice = [self.delim_char];
        let delim_str: &str = from_utf8(&delim_slice)?;
        let mut fields = vec![Cow::Borrowed(delim_str); 32];
        let _ = split(&mut fields, header, self.delim_char, self.quote_char)?;

        self.headers = fields
            .into_iter()
            .filter(|i| i != delim_str)
            .map(String::from)
            .collect();
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
            buffer = &buffer[..buffer.len() - 1];
        }
        if buffer.last() == Some(&b'\r') {
            buffer = &buffer[..buffer.len() - 1];
        }
        let mut records = vec![Cow::Borrowed(""); state.headers.len()];
        let n_records = split(&mut records, buffer, state.delim_char, state.quote_char)?;
        if n_records != state.headers.len() {
            return Err("Line had a bad number of records".into());
        }
        if let Some(types) = &state.types {
            self.values = records
                .into_iter()
                .zip(types)
                .map(|(v, ty)| ty.coerce(v))
                .collect();
        } else {
            self.values = records.into_iter().map(Value::from).collect();
        }
        Ok(())
    }
}

impl<'r> From<TsvRecord<'r>> for Vec<Value<'r>> {
    fn from(record: TsvRecord<'r>) -> Self {
        record.values
    }
}

impl_reader!(TsvReader, TsvRecord, TsvRecord<'r>, TsvState, TsvParams);

#[cfg(test)]
mod test {
    use super::*;

    use crate::readers::RecordReader;

    #[test]
    fn test_reader() -> Result<(), EtError> {
        const TEST_TEXT: &[u8] = b"header\nrow\nanother row";
        let mut pt = TsvReader::new(TEST_TEXT, Some(TsvParams::default()))?;

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
        let mut pt = TsvReader::new(TEST_TEXT, Some(TsvParams::default()))?;

        assert_eq!(&pt.headers(), &["header", "col1"]);
        let mut ix = 0;
        while let Some(TsvRecord { values }) = pt.next()? {
            match ix {
                0 => {
                    assert_eq!(values.len(), 2);
                    assert_eq!(values[0], "row".into());
                    assert_eq!(values[1], 2.into());
                }
                1 => {
                    assert_eq!(values.len(), 2);
                    assert_eq!(values[0], "another row".into());
                    assert_eq!(values[1], 3.into());
                }
                _ => return Err("bad line".into()),
            }
            ix += 1;
        }
        assert_eq!(ix, 2);
        Ok(())
    }

    #[test]
    fn test_bad_fuzzes() -> Result<(), EtError> {
        const TEST_TEXT: &[u8] = b"U,\n\n\n";
        let mut pt = TsvReader::new(TEST_TEXT, None)?;
        while let Some(TsvRecord { values: _ }) = pt.next()? {}

        const TEST_TEXT_2: &[u8] = b"U\n2\n2:";
        let mut pt = TsvReader::new(TEST_TEXT_2, None)?;
        while let Some(TsvRecord { values: _ }) = pt.next()? {}

        Ok(())
    }
}
