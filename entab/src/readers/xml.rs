use core::marker::Copy;
// use alloc::collections::BTreeMap;
use alloc::str::from_utf8;

use memchr::{memchr, memchr3_iter};

use crate::buffer::ReadBuffer;
use crate::parsers::FromBuffer;
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
    SelfClose
}
// TODO: maybe CDATA, DOCTYPE, comments too?

/// Convenience struct for tokenizing tags out of XML streams
#[derive(Clone, Copy, Debug)]
pub struct XmlTag<'r> {
    tag_type: XmlTagType,
    id: &'r str
}

impl<'r> FromBuffer<'r> for XmlTag<'r> {
    type State = ();

    fn get(rb: &'r mut ReadBuffer, _state: Self::State) -> Result<Self, EtError> {
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
                return Err(EtError::new(format!("Tags larger than {} not supported", 1024)));
            }
            if rb.eof() {
                return Err(EtError::new("Tag was never closed"));
            }
            start = rb.len() - 1;
            rb.refill()?;
        };
        let data = rb.consume(end);

        let is_closing = data[1] == b'/';
        let is_self_closing = data.last() == Some(&b'/');
        let (tag_type, data) = match (is_closing, is_self_closing) {
            (true, true) => return Err(EtError::new("Tag can not start and end with '/'")),
            (true, false) => (XmlTagType::Close, &data[2..data.len() - 1]),
            (false, true) => (XmlTagType::SelfClose, &data[1..data.len() - 2]),
            (false, false) => (XmlTagType::Open, &data[1..data.len() - 1]),
        };
        let id_end = memchr(b' ', data).unwrap_or(data.len());
        let id = from_utf8(&data[..id_end])?;
        // TODO: parse attributes

        Ok(XmlTag {
            tag_type,
            id,
        })
    }
}

/// Convenience struct for tokenizing text out of XML streams
#[derive(Clone, Copy, Debug)]
pub struct XmlText<'r>(&'r str);

impl<'r> FromBuffer<'r> for XmlText<'r> {
    type State = ();

    fn get(rb: &'r mut ReadBuffer, _state: Self::State) -> Result<Self, EtError> {
        let mut start = 0;
        let end = loop {
            // we're parsing a text element
            if let Some(e) = memchr(b'<', &rb[start..]) {
                break e;
            }
            if rb.len() > 65536 {
                return Err(EtError::new(format!("XML text larger than {} not supported", 65536)));
            }
            if rb.eof() {
                // TODO: add test for this case
                break rb.len();
            }
            start = rb.len() - 1;
            rb.refill()?;
        };
        Ok(XmlText(from_utf8(rb.consume(end))?))
    }
}

/// Current state of the XML parser
#[derive(Clone, Debug)]
pub struct XmlState {
    // token_counts: Vec<BTreeMap<String, usize>>,
    stack: Vec<String>,
}

impl<'r> StateMetadata<'r> for XmlState {}

impl<'r> FromBuffer<'r> for XmlState {
    type State = ();

    fn get(_rb: &'r mut ReadBuffer, _state: Self::State) -> Result<Self, EtError> {
        Ok(XmlState {
            // token_counts: Vec::new(),
            stack: Vec::new(),
        })
    }
}

/// A single record from an XML stream
#[derive(Clone, Debug)]
pub struct XmlRecord<'r> {
    tags: &'r [String],
    text: &'r str,
    // TODO
    // attributes: BTreeMap<String>
}

impl<'r> FromBuffer<'r> for Option<XmlRecord<'r>> {
    type State = &'r mut XmlState;

    fn get(rb: &'r mut ReadBuffer, state: Self::State) -> Result<Self, EtError> {
        if rb.is_empty() {
            if !state.stack.is_empty() {
                return Err(EtError::new(format!("Closing tag for {} not present?", state.stack.pop().unwrap())));
            } else {
                return Ok(None);
            }
        }
        let text = if rb[0] == b'<' {
            // it's a tag
            let tag = rb.extract::<XmlTag>(())?;
            match tag.tag_type {
                XmlTagType::Open => {
                    state.stack.push(tag.id.to_owned());
                },
                XmlTagType::Close => {
                    if let Some(open_tag) = state.stack.pop() {
                        if open_tag != tag.id {
                            return Err(EtError::new(format!("Closing tag {} found, but {} was open.", tag.id, open_tag)));
                        }
                    } else {
                        return Err(EtError::new(format!("Closing tag {} found, but no tags opened before it.", tag.id)));
                    }
                },
                // TODO: we need to return the tag stack with this tag on it
                XmlTagType::SelfClose => {},
            }
            ""
        } else {
            // it's text
            let text = rb.extract::<XmlText>(())?;
            text.0
        };

        Ok(Some(XmlRecord {
            tags: &state.stack,
            text,
        }))
    }
}

impl_record!(XmlRecord<'r>: tags, text);

impl_reader!(XmlReader, XmlRecord, XmlState, ());

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xml_reader() -> Result<(), EtError> {
        let rb = ReadBuffer::from_slice(b"<a>test</a>");
        let mut reader = XmlReader::new(rb, ())?;

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
