use std::convert::Into;
use std::io::Write;

use entab::error::EtError;
use entab::record::Value;

use memchr::{memchr, memchr_iter};

#[allow(dead_code)] // TODO: add support for this into the CLI client
pub enum TsvEscapeStyle {
    Quote(u8),
    Escape(u8),
    Replace(u8),
}

pub struct TsvParams {
    pub null_value: Vec<u8>,
    pub true_value: Vec<u8>,
    pub false_value: Vec<u8>,
    pub line_delimiter: Vec<u8>,
    pub main_delimiter: u8,
    pub escape_style: TsvEscapeStyle,
    pub list_delimiter: u8,
    pub list_start_end: (Vec<u8>, Vec<u8>),
    pub record_delimiter: u8,
}

impl Default for TsvParams {
    fn default() -> Self {
        TsvParams {
            null_value: b"null".to_vec(),
            true_value: b"true".to_vec(),
            false_value: b"false".to_vec(),
            line_delimiter: vec![b'\n'],
            main_delimiter: b'\t',
            escape_style: TsvEscapeStyle::Quote(b'"'),
            list_delimiter: b',',
            list_start_end: (b"".to_vec(), b"".to_vec()),
            record_delimiter: b':',
        }
    }
}

impl TsvParams {
    pub fn write_str<W>(&self, string: &'_ [u8], mut writer: W) -> Result<(), EtError>
    where
        W: Write,
    {
        let first = match memchr(self.main_delimiter, string) {
            Some(break_loc) => break_loc,
            None => {
                return writer.write_all(string).map_err(Into::into);
            }
        };
        if let TsvEscapeStyle::Quote(quote_char) = self.escape_style {
            writer.write_all(&[quote_char])?;
            writer.write_all(string)?;
            return writer.write_all(&[quote_char]).map_err(Into::into);
        };
        writer.write_all(&string[..first])?;
        if let TsvEscapeStyle::Escape(escape_char) = self.escape_style {
            writer.write_all(&[escape_char, self.main_delimiter])?;
        } else if let TsvEscapeStyle::Replace(replace_char) = self.escape_style {
            writer.write_all(&[replace_char])?;
        }
        let mut old_pos = 1;
        for pos in memchr_iter(self.main_delimiter, &string[first + 1..]) {
            writer.write_all(&string[first + old_pos..=first + pos])?;
            if let TsvEscapeStyle::Escape(escape_char) = self.escape_style {
                writer.write_all(&[escape_char, self.main_delimiter])?;
            } else if let TsvEscapeStyle::Replace(replace_char) = self.escape_style {
                writer.write_all(&[replace_char])?;
            }
            old_pos = pos + 2;
        }
        if old_pos < string.len() {
            writer.write_all(&string[first + old_pos..])?;
        }
        Ok(())
    }

    /// Write a `Value` out to a TSV stream
    pub fn write_value<W>(&self, value: &Value, mut writer: &mut W) -> Result<(), EtError>
    where
        W: Write,
    {
        match value {
            Value::Null => writer.write_all(&self.null_value)?,
            Value::Boolean(true) => writer.write_all(&self.true_value)?,
            Value::Boolean(false) => writer.write_all(&self.false_value)?,
            Value::Datetime(s) => writer.write_all(format!("{:+?}", s).as_bytes())?,
            Value::Float(v) => writer.write_all(format!("{}", v).as_bytes())?,
            Value::Integer(v) => writer.write_all(format!("{}", v).as_bytes())?,
            Value::List(l) => {
                writer.write_all(&self.list_start_end.0)?;
                if !l.is_empty() {
                    self.write_value(&l[0], writer)?;
                    for i in &l[1..] {
                        writer.write_all(&[self.list_delimiter])?;
                        self.write_value(i, writer)?;
                    }
                }
                writer.write_all(&self.list_start_end.1)?;
            }
            Value::Record(_) => unimplemented!("No writer for records yet"),
            Value::String(s) => self.write_str(s.as_bytes(), &mut writer)?,
        };
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_replace_chars() {
        let mut params = TsvParams::default();
        params.escape_style = TsvEscapeStyle::Replace(b'|');

        let mut buffer = Cursor::new(Vec::new());
        let _ = params.write_str(b"", &mut buffer);
        assert_eq!(buffer.get_ref(), b"");

        let mut buffer = Cursor::new(Vec::new());
        let _ = params.write_str(b"\t", &mut buffer);
        assert_eq!(buffer.get_ref(), b"|");

        let mut buffer = Cursor::new(Vec::new());
        let _ = params.write_str(b"test", &mut buffer);
        assert_eq!(buffer.get_ref(), b"test");

        let mut buffer = Cursor::new(Vec::new());
        let _ = params.write_str(b"\ttest", &mut buffer);
        assert_eq!(buffer.get_ref(), b"|test");

        let mut buffer = Cursor::new(Vec::new());
        let _ = params.write_str(b"\ttest\t", &mut buffer);
        assert_eq!(buffer.get_ref(), b"|test|");

        let mut buffer = Cursor::new(Vec::new());
        let _ = params.write_str(b"\ttest\tt\t", &mut buffer);
        assert_eq!(buffer.get_ref(), b"|test|t|");

        let mut buffer = Cursor::new(Vec::new());
        let _ = params.write_str(b"\t\t\t", &mut buffer);
        assert_eq!(buffer.get_ref(), b"|||");
    }

    #[test]
    fn test_escape_params() {
        let mut params = TsvParams::default();
        params.escape_style = TsvEscapeStyle::Escape(b'|');

        let mut buffer = Cursor::new(Vec::new());
        let _ = params.write_str(b"\t", &mut buffer);
        assert_eq!(buffer.get_ref(), b"|\t");

        let mut buffer = Cursor::new(Vec::new());
        let _ = params.write_str(b"\ttest\t", &mut buffer);
        assert_eq!(buffer.get_ref(), b"|\ttest|\t");
    }

    #[test]
    fn test_write_value_date() -> Result<(), EtError> {
        const DATE: &str = "2001-02-03T04:05:06.000Z";
        const OUT_DATE: &[u8] = b"2001-02-03T04:05:06";

        let p = TsvParams::default();
        let mut buffer = Cursor::new(Vec::new());
        let datetime = Value::from_iso_date(DATE)?;
        let _ = p.write_value(&datetime, &mut buffer);
        assert_eq!(buffer.get_ref(), &OUT_DATE);
        Ok(())
    }
}
