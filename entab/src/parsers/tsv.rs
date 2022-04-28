use alloc::borrow::Cow;
use alloc::str::from_utf8;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use memchr::memchr;

use crate::impl_reader;
use crate::parsers::common::NewLine;
use crate::parsers::FromSlice;
use crate::parsers::{extract, extract_opt};
use crate::record::{StateMetadata, Value};
use crate::EtError;

/// Split a line into fields. Fields are separated by `delim` unless the field is surrounded by
/// `quote`. This parser requires that if a field is quoted then the quotes must be directly next
/// to the neighboring `delim`s (some more lenient parsers allow whitespace between).
#[inline]
pub(crate) fn split<'a>(
    buffer: &mut Vec<Cow<'a, str>>,
    line: &'a [u8],
    delim: u8,
    quote: u8,
) -> Result<usize, EtError> {
    let mut cur_pos = 0;
    let mut token_num = 0;
    while cur_pos < line.len() {
        if token_num >= buffer.len() {
            buffer.push(Cow::Borrowed(""));
        }
        if line[cur_pos] == quote {
            let mut quoted_quotes = false;
            loop {
                if let Some(next) = memchr(quote, &line[cur_pos + 1..]) {
                    if cur_pos + next + 2 == line.len() || line[cur_pos + next + 2] == delim {
                        // either the next quote is right before a delimiter
                        if quoted_quotes {
                            buffer[token_num] += from_utf8(&line[cur_pos + 1..cur_pos + next + 1])?;
                        } else {
                            buffer[token_num] =
                                Cow::Borrowed(from_utf8(&line[cur_pos + 1..cur_pos + next + 1])?);
                        }
                        cur_pos += next + 2;
                        break;
                    } else if line[cur_pos + next + 2] != quote {
                        return Err("quotes must start and end next to delimiters".into());
                    }
                    // or its right before a pair of quotes (how CSVs escape a quote inside quoted
                    // output). note that the error case is above because we need to continue
                    // parsing quotes if we're in the pair scenario.
                    if quoted_quotes {
                        buffer[token_num] += from_utf8(&line[cur_pos + 1..cur_pos + next + 2])?;
                    } else {
                        buffer[token_num] =
                            Cow::Borrowed(from_utf8(&line[cur_pos + 1..cur_pos + next + 2])?);
                    }
                    quoted_quotes = true;
                    cur_pos += next + 2;
                } else {
                    return Err("unclosed delimiter".into());
                }
            }
        } else if let Some(next) = memchr(delim, &line[cur_pos..]) {
            buffer[token_num] = from_utf8(&line[cur_pos..cur_pos + next])?.into();
            cur_pos += next;
        } else {
            buffer[token_num] = from_utf8(&line[cur_pos..line.len()])?.into();
            cur_pos = line.len();
        };
        cur_pos += 1;
        token_num += 1;
    }
    buffer.truncate(token_num);
    Ok(token_num)
}

const DELIMS: &[u8] = b"\t;:|~,^ ";
const N_DELIMS: usize = 9;

fn count_bytes(line: &[u8], stats: &mut [StreamingStats; N_DELIMS], quote_diff: &mut i32) {
    let mut counts = [0u16; N_DELIMS];
    for b in line {
        counts[match b {
            // possible delimiters
            b'\t' => 0,
            b';' => 1,
            b':' => 2,
            b'|' => 3,
            b'~' => 4,
            b',' => 5,
            b'^' => 6,
            b' ' => 7,
            b'\'' => {
                *quote_diff = quote_diff.saturating_sub(1);
                8
            }
            b'"' => {
                *quote_diff = quote_diff.saturating_add(1);
                8
            }
            // everything else
            _ => 8,
        }] += 1;
    }
    for (count, stat) in counts.iter().zip(stats.iter_mut()) {
        stat.update(*count as f64);
    }
}

/// Used to compute basic statistics on streaming data
#[derive(Clone, Copy, Debug, Default)]
pub struct StreamingStats {
    n: usize,
    mean: f64,
    m2: f64,
    min: f64,
    max: f64,
}

impl StreamingStats {
    /// Create a fresh `StreamingStats` struct
    pub fn new() -> Self {
        StreamingStats {
            n: 0,
            mean: 0.,
            m2: 0.,
            min: f64::MAX,
            max: f64::MIN,
        }
    }

    /// Update `StreamingStats` with a new value
    pub fn update(&mut self, val: f64) {
        self.n += 1;

        // update the mean/std dev trackers
        let delta = val - self.mean;
        self.mean += delta / self.n as f64;
        let delta2 = val - self.mean;
        self.m2 += delta * delta2;

        // update the min/max
        self.min = self.min.min(val);
        self.max = self.max.max(val);
    }

    /// Return the current variance
    pub fn variance(&self) -> f64 {
        self.m2 / self.n as f64
    }
}

// decimal separator
// date format
// text encoding
// striping spaces around values?
// n preface lines
// header line?
//
// RFC: https://datatracker.ietf.org/doc/html/rfc4180
// Frictionless data spec: https://specs.frictionlessdata.io//csv-dialect/

/// Parameters for parsing TSVs
#[derive(Clone, Copy, Debug)]
pub struct TsvParams {
    delim_char: Option<u8>,
    quote_char: Option<u8>,
    skip_lines: Option<usize>,
    sniff_file: bool,
}

impl Default for TsvParams {
    fn default() -> Self {
        TsvParams {
            delim_char: None,
            quote_char: None,
            skip_lines: None,
            sniff_file: true,
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

    fn sniff_from_data(&mut self, data: &[u8]) {
        let con = &mut 0;
        let mut stats = [StreamingStats::new(); N_DELIMS];
        let mut quote_diff = 0;
        while let Ok(NewLine(line)) = extract(data, con, &mut 0) {
            count_bytes(line, &mut stats, &mut quote_diff);
        }

        if self.quote_char.is_none() {
            self.quote_char = Some(if quote_diff < 0 { b'\'' } else { b'"' });
        }

        let mut possible_delims = Vec::new();
        for (ix, stat) in stats.iter().take(N_DELIMS - 1).enumerate() {
            let avg_delims_required = if DELIMS[ix] == b' ' {
                3. // we have a higher bar for spaces because they're uncommon as a delimiter
            } else {
                1.
            };
            if stat.mean >= avg_delims_required {
                possible_delims.push((stat.variance(), stat.mean, DELIMS[ix]));
            }
        }
        // we're not comparing with `mean` because it's possible that fields could have more commas
        // than tabs if it's they're being used as a decimal (european) like `1,0\t2,0\t3,0`
        possible_delims.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(core::cmp::Ordering::Equal));
        let (delim_char, avg_delims) = if possible_delims.is_empty() {
            (b',', 0.)
        } else {
            (possible_delims[0].2, possible_delims[0].1)
        };
        if self.delim_char.is_none() {
            self.delim_char = Some(delim_char);
        }

        // try to guess how many lines of comments are at the top
        let con = &mut 0;
        let mut ix = 0;
        let mut skip_lines = 0;
        let mut in_data = 0;
        while let Ok(NewLine(line)) = extract(data, con, &mut 0) {
            if ix > 100 {
                // we're not finding a pattern so just abort and keep the 0
                break;
            }
            let n_delims = line.iter().filter(|b| *b == &delim_char).count();
            if (n_delims as f64 - avg_delims).abs() < 1. {
                if in_data == 0 {
                    skip_lines = ix;
                } else if in_data == 5 {
                    break;
                }
                in_data += 1;
            } else {
                in_data = 0;
            }
            ix += 1;
        }
        if self.skip_lines.is_none() {
            self.skip_lines = Some(skip_lines);
        }
    }
}

/// Track the current state of the TSV parser
#[derive(Clone, Debug, Default)]
pub struct TsvState {
    headers: Vec<String>,
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
            state.sniff_from_data(buffer);
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

        self.delim_char = state.delim_char.unwrap_or(b'\t');
        self.quote_char = state.quote_char.unwrap_or(b'"');

        // prefill with something impossible so we can tell how big the header is
        let delim_str = [self.delim_char];
        let mut buffer = vec![Cow::Borrowed(from_utf8(&delim_str)?); 32];
        let _ = split(&mut buffer, header, self.delim_char, self.quote_char)?;

        self.headers = buffer
            .into_iter()
            .filter(|i| i != "\t")
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
        self.values = records.into_iter().map(Value::from).collect();
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
    fn test_split() -> Result<(), EtError> {
        let mut buffer = Vec::new();

        assert_eq!(split(&mut buffer, b"1,2,3,4", b',', b'"')?, 4);
        assert_eq!(&buffer, &["1", "2", "3", "4"]);

        assert_eq!(split(&mut buffer, b"1,\"2,3\",4", b',', b'"')?, 3);
        assert_eq!(&buffer, &["1", "2,3", "4"]);

        assert_eq!(split(&mut buffer, b"1,\"2,\"\"3\"\"\",4", b',', b'"')?, 3);
        assert_eq!(&buffer, &["1", "2,\"3\"", "4"]);

        assert_eq!(
            split(&mut buffer, b"1,\"\"\"2\"\",\"\"3\"\"\",4", b',', b'"')?,
            3
        );
        assert_eq!(&buffer, &["1", "\"2\",\"3\"", "4"]);

        assert_eq!(split(&mut buffer, b"\"\"\"\"\"2\"\"\"\"\"", b',', b'"')?, 1);
        assert_eq!(&buffer, &["\"\"2\"\""]);

        assert!(split(&mut buffer, b"\"", b',', b'"').is_err());
        assert!(split(&mut buffer, b"\"\" ,2", b',', b'"').is_err());

        Ok(())
    }

    #[test]
    fn test_sniff_params() -> Result<(), EtError> {
        let mut params = TsvParams::default();
        params.sniff_from_data(b"test\tthis\theader\n1\t2\t3");
        assert_eq!(params.delim_char, Some(b'\t'));
        assert_eq!(params.quote_char, Some(b'"'));
        assert_eq!(params.skip_lines, Some(0));

        let mut params = TsvParams::default();
        params.sniff_from_data(b"1,0|2,0|3,0\n4,0|5,0|6,0");
        assert_eq!(params.delim_char, Some(b'|'));
        assert_eq!(params.quote_char, Some(b'"'));
        assert_eq!(params.skip_lines, Some(0));

        let mut params = TsvParams::default();
        params.sniff_from_data(b"this is a comment\n1,2,'a'\n4,5,'b'\n6,7,'c'");
        assert_eq!(params.delim_char, Some(b','));
        assert_eq!(params.quote_char, Some(b'\''));
        assert_eq!(params.skip_lines, Some(1));
        Ok(())
    }

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

    #[test]
    fn test_bad_fuzzes() -> Result<(), EtError> {
        const TEST_TEXT: &[u8] = b"U,\n\n\n";
        let mut pt = TsvReader::new(TEST_TEXT, Some(TsvParams::default()))?;
        while let Some(TsvRecord { values }) = pt.next()? {}

        Ok(())
    }
}
