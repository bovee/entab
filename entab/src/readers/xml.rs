use core::marker::Copy;
// use alloc::collections::BTreeMap;
use alloc::borrow::ToOwned;
use alloc::format;
use alloc::str::from_utf8;
use alloc::string::String;
use alloc::vec::Vec;

use memchr::{memchr, memchr3_iter};

use crate::parsers::{extract, FromSlice};
use crate::record::StateMetadata;
use crate::EtError;
use crate::{impl_reader, impl_record};

/// What kind of XML tag this is
#[derive(Clone, Copy, Debug)]
pub enum XmlTagType {
    /// An opening tag, e.g. <a>
    Open,
    /// An closing tag, e.g. </a>
    Close,
    /// A self-closing tag, e.g. <br />
    SelfClose,
}
// TODO: maybe CDATA, DOCTYPE, comments too?

impl Default for XmlTagType {
    fn default() -> Self {
        XmlTagType::Open
    }
}

/// Convenience struct for tokenizing tags out of XML streams
#[derive(Clone, Copy, Debug, Default)]
pub struct XmlTag<'r> {
    tag_type: XmlTagType,
    id: &'r str,
}

impl<'r> FromSlice<'r> for XmlTag<'r> {
    type State = ();

    fn parse(
        rb: &[u8],
        eof: bool,
        consumed: &mut usize,
        _state: &mut Self::State,
    ) -> Result<bool, EtError> {
        let con = &mut 0;
        let mut cur_quote = b' ';
        let mut start = 0;
        let end = 'read: loop {
            // we're parsing a tag
            for i in memchr3_iter(b'>', b'"', b'\'', &rb[start..]) {
                match (rb[i], cur_quote) {
                    // if we're not in quotes and see a >, break
                    (b'>', b' ') => break 'read i + 1,
                    // if we're not in quotes and see a quote, start "quoting"
                    (b'\'', b' ') => cur_quote = b'\'',
                    (b'"', b' ') => cur_quote = b'"',
                    // if we're in quotes and see a quote, stop "quoting"
                    (b'\'', b'\'') => cur_quote = b' ',
                    (b'"', b'"') => cur_quote = b' ',
                    _ => {}
                }
            }
            if rb.len() > 1024 {
                return Err(format!("Tags larger than {} not supported", 1024).into());
            }
            if eof {
                return Err("Tag was never closed".into());
            }
            start = rb.len() - 1;
            return Ok(false);
        };
        *con += end;
        Ok(true)
    }

    fn get(
        &mut self,
        buf: &'r [u8],
        state: &Self::State,
    ) -> Result<(), EtError> {
        let is_closing = buf[1] == b'/';
        let is_self_closing = buf.last() == Some(&b'/');
        let (tag_type, data) = match (is_closing, is_self_closing) {
            // TODO: we should be able to use EtError::new here
            (true, true) => return Err(EtError::from("Tag can not start and end with '/'")),
            (true, false) => (XmlTagType::Close, &buf[2..buf.len() - 1]),
            (false, true) => (XmlTagType::SelfClose, &buf[1..buf.len() - 2]),
            (false, false) => (XmlTagType::Open, &buf[1..buf.len() - 1]),
        };
        let id_end = memchr(b' ', data).unwrap_or(data.len());
        self.tag_type = tag_type;
        self.id = from_utf8(&data[..id_end])?;
        // TODO: parse attributes
        Ok(())
    }
}

/// Convenience struct for tokenizing text out of XML streams
#[derive(Clone, Copy, Debug, Default)]
pub struct XmlText<'r>(&'r str);

impl<'r> FromSlice<'r> for XmlText<'r> {
    type State = ();

    fn parse(
        rb: &[u8],
        eof: bool,
        consumed: &mut usize,
        _state: &mut Self::State,
    ) -> Result<bool, EtError> {
        let mut start = 0;
        let end = loop {
            // we're parsing a text element
            if let Some(e) = memchr(b'<', &rb[start..]) {
                break e;
            }
            if rb.len() > 65536 {
                return Err(
                    format!("XML text larger than {} not supported", 65536).into()
                );
            }
            if eof {
                // TODO: add test for this case
                break rb.len();
            }
            start = rb.len() - 1;
            return Ok(false);
        };
        *consumed += end;
        Ok(true)
    }

    fn get(
        &mut self,
        buf: &'r [u8],
        state: &Self::State,
    ) -> Result<(), EtError> {
        self.0 = from_utf8(buf)?;
        Ok(())
    }
}

/// Current state of the XML parser
#[derive(Clone, Debug, Default)]
pub struct XmlState {
    // token_counts: Vec<BTreeMap<String, usize>>,
    stack: Vec<String>,
    is_text: bool,
}

impl StateMetadata for XmlState {}

impl<'r> FromSlice<'r> for XmlState {
    type State = ();
}

/// A single record from an XML stream
#[derive(Clone, Debug, Default)]
pub struct XmlRecord<'r> {
    tags: Vec<String>,
    text: &'r str,
    // TODO
    // attributes: BTreeMap<String>
}

impl<'r> FromSlice<'r> for XmlRecord<'r> {
    type State = &'r mut XmlState;

    fn parse(rb: &[u8], eof: bool, consumed: &mut usize, state: &mut Self::State) -> Result<bool, EtError> {
        if rb.is_empty() {
            if !state.stack.is_empty() {
                return Err(format!("Closing tag for {} not present?", state.stack.pop().unwrap()).into());
            } else {
                return Ok(false);
            }
        }
        let con = &mut 0;
        if rb[0] == b'<' {
            // it's a tag
            let tag = extract::<XmlTag>(rb, con, ())?;
            match tag.tag_type {
                XmlTagType::Open => {
                    state.stack.push(tag.id.to_owned());
                }
                XmlTagType::Close => {
                    if let Some(open_tag) = state.stack.pop() {
                        if open_tag != tag.id {
                            return Err(
                                format!("Closing tag {} found, but {} was open.", tag.id, open_tag).into()
                            );
                        }
                    } else {
                        return Err(
                            format!(
                                "Closing tag {} found, but no tags opened before it.",
                                tag.id
                            ).into()
                        );
                    }
                }
                // TODO: we need to return the tag stack with this tag on it
                XmlTagType::SelfClose => {}
            }
            state.is_text = false;
        } else {
            // it's text; parse the length out
            if XmlText::parse(rb, eof, con, &mut ())? {
                state.is_text = true;
            } else {
                return Ok(false);
            }
        }
        *consumed += *con;

        Ok(true)
    }

    fn get(
        &mut self,
        rb: &'r [u8],
        state: &Self::State,
    ) -> Result<(), EtError> {
        self.text = if state.is_text {
            from_utf8(rb)?
        } else {
            ""
        };
        self.tags = state.stack.clone();
        Ok(())
    }
}

impl_record!(XmlRecord<'r>: tags, text);

impl_reader!(XmlReader, XmlRecord, XmlState, ());

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xml_reader() -> Result<(), EtError> {
        let data: &[u8] = b"<a>test</a>";
        let mut reader = XmlReader::new(data, ())?;

        // TODO: don't emit on tag close? also emit the current tag?
        let rec = reader.next()?.unwrap();
        assert_eq!(rec.tags, &["a"]);
        let rec = reader.next()?.unwrap();
        assert_eq!(rec.tags, &["a"]);
        let rec = reader.next()?.unwrap();
        assert!(rec.tags.is_empty());
        assert!(reader.next()?.is_none());
        Ok(())
    }
}
