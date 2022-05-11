use alloc::borrow::Cow;
use alloc::str::from_utf8;
use alloc::vec::Vec;
use alloc::vec;

use memchr::memchr;

use crate::error::EtError;
use crate::parsers::common::NewLine;
use crate::parsers::extract;
use crate::parsers::tsv::TsvParams;
use crate::record::Value;

/// Used to compute basic statistics on streaming data
#[derive(Clone, Copy, Debug, Default)]
pub struct StreamingStats {
    /// The number of records streamed.
    pub n: usize,
    /// The average/mean of the records seen so far.
    pub mean: f64,
    m2: f64,
    /// The smallest value so far.
    pub min: f64,
    /// The largest value so far.
    pub max: f64,
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
    // special case if there's a null record at the very end of the line
    if line.last() == Some(&delim) {
        if token_num >= buffer.len() {
            buffer.push(Cow::Borrowed(""));
        } else {
            buffer[token_num] = "".into();
        }
        token_num += 1;
    }
    buffer.truncate(token_num);
    Ok(token_num)
}

/// Determine the delimiter, quoting character, and number of comment lines to skip.
pub fn sniff_params_from_data(params: &mut TsvParams, data: &[u8]) {
    let con = &mut 0;
    let mut stats = [StreamingStats::new(); N_DELIMS];
    let mut quote_diff = 0;
    while let Ok(NewLine(line)) = extract(data, con, &mut 0) {
        count_bytes(line, &mut stats, &mut quote_diff);
    }

    if params.quote_char.is_none() {
        params.quote_char = Some(if quote_diff < 0 { b'\'' } else { b'"' });
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
    if params.delim_char.is_none() {
        params.delim_char = Some(delim_char);
    }

    // try to guess how many lines of comments are at the top
    let con = &mut 0;
    let mut ix = 0;
    let mut skip_lines = 0;
    let mut in_data = 0;
    while let Ok(NewLine(line)) = extract(data, con, &mut 0) {
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
    if params.skip_lines.is_none() {
        params.skip_lines = Some(skip_lines);
    }
}

/// Determine the types of the fields in the data.
pub fn sniff_types_from_data(params: &mut TsvParams, data: &[u8]) {
    let delim_char = params.delim_char.unwrap_or(DEFAULT_DELIM);
    let mut fields = vec![Cow::Borrowed(""); 32];
    let mut types: Vec<TsvFieldType> = Vec::new();
    let mut line_ix = 0;
    let con = &mut 0;
    while let Ok(NewLine(line)) = extract(data, con, &mut 0) {
        // TODO: + 1 for the "headers" line; this should probably be configurable
        if line_ix < params.skip_lines.unwrap_or(0) + 1 {
            line_ix += 1;
            continue;
        }
        let _ = split(
            &mut fields,
            line,
            delim_char,
            params.quote_char.unwrap_or(b'"'),
        );
        for (field_ix, field) in fields.iter().enumerate() {
            if field_ix >= types.len() {
                let mut ty = TsvFieldType::default();
                ty.infer(field);
                types.push(ty);
            } else {
                types[field_ix].infer(field);
            }
        }
        line_ix += 1;
    }
    params.types = types;
}

const DELIMS: &[u8] = b"\t;:|~,^ ";
const N_DELIMS: usize = 9;

/// The default delimiter if one is not provided.
pub const DEFAULT_DELIM: u8 = b'\t';
/// The default quoting character if one is not provided.
pub const DEFAULT_QUOTE: u8 = b'"';

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

const TSV_STR: u8 = 1;
const TSV_BOOL: u8 = 2;
const TSV_FLOAT: u8 = 4;
const TSV_INT: u8 = 8;
const TSV_DATE: u8 = 16;

/// The type of a TSV field
#[derive(Clone, Copy, Debug)]
pub struct TsvFieldType {
    ty: u8,
}

impl Default for TsvFieldType {
    fn default() -> Self {
        TsvFieldType { ty: u8::MAX }
    }
}

impl TsvFieldType {
    /// Infer the type of a given string and update self
    pub fn infer(&mut self, field: &str) {
        let mut possible_type = TSV_STR;
        let field = field.trim();
        if field == "F"
            || field == "f"
            || field == "FALSE"
            || field == "false"
            || field == "False"
            || field == "T"
            || field == "t"
            || field == "TRUE"
            || field == "true"
            || field == "True"
        {
            possible_type |= TSV_BOOL;
        }

        let mut numeric = false;
        let mut nonnumeric = false;
        let mut has_period = false;
        let mut has_comma = false;
        for chr in field.chars() {
            match chr {
                '0'..='9' => numeric = true,
                '.' => has_period = true,
                ',' => has_comma = true,
                ' ' | '+' | '-' => {}
                _ => nonnumeric = true,
            }
        }
        if numeric && !nonnumeric {
            if has_comma || has_period {
                possible_type |= TSV_FLOAT;
            } else if !(has_comma || has_period) {
                possible_type |= TSV_INT;
            }
        }

        // TODO: check for dates?
        self.ty &= possible_type;
    }

    /// Coerce a string into a Value
    pub fn coerce<'a>(&self, field: Cow<'a, str>) -> Value<'a> {
        let f = field.trim();
        match 128 >> self.ty.leading_zeros() {
            // TODO: we can't use `trim` because that requires a borrow inside this function :/
            TSV_STR => Value::from(field),
            TSV_BOOL => {
                if f == "T" || f == "t" || f == "TRUE" || f == "True" || f == "true" {
                    Value::Boolean(true)
                } else {
                    Value::Boolean(false)
                }
            }
            TSV_FLOAT => f
                .parse::<f64>()
                .map_or_else(|_| Value::from(field), Value::from),
            TSV_INT => f
                .parse::<i64>()
                .map_or_else(|_| Value::from(field), Value::from),
            // TODO: handle dates
            TSV_DATE => Value::from(field),
            _ => Value::from(field),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use crate::error::EtError;

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
        sniff_params_from_data(&mut params, b"test\tthis\theader\n1\t2\t3");
        assert_eq!(params.delim_char, Some(b'\t'));
        assert_eq!(params.quote_char, Some(b'"'));
        assert_eq!(params.skip_lines, Some(0));

        let mut params = TsvParams::default();
        sniff_params_from_data(&mut params, b"1,0|2,0|3,0\n4,0|5,0|6,0");
        assert_eq!(params.delim_char, Some(b'|'));
        assert_eq!(params.quote_char, Some(b'"'));
        assert_eq!(params.skip_lines, Some(0));

        let mut params = TsvParams::default();
        sniff_params_from_data(&mut params, b"this is a comment\n1,2,'a'\n4,5,'b'\n6,7,'c'");
        assert_eq!(params.delim_char, Some(b','));
        assert_eq!(params.quote_char, Some(b'\''));
        assert_eq!(params.skip_lines, Some(1));
        Ok(())
    }
}
