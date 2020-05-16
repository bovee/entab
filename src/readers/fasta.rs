use std::borrow::Cow;
use std::io::Write;

use memchr::memchr;

use crate::buffer::ReadBuffer;
use crate::record::{BindT, ReaderBuilder, Record, RecordReader};
use crate::EtError;

pub fn replace_tabs(slice: &[u8]) -> Cow<[u8]> {
    static REPLACE_CHAR: u8 = b'|';

    if let Some(p) = memchr(b'\t', slice) {
        let mut new_slice = slice.to_vec();
        for c in new_slice[p..].iter_mut() {
            if *c == b'\t' {
                *c = REPLACE_CHAR;
            }
        }
        new_slice.into()
    } else {
        slice.into()
    }
}

#[derive(Debug)]
pub struct FastaRecord<'s> {
    id: &'s str,
    sequence: Cow<'s, [u8]>,
}

impl<'s> Record for FastaRecord<'s> {
    fn size(&self) -> usize {
        2
    }

    fn write_field(&self, num: usize, writer: &mut dyn Write) -> Result<(), EtError> {
        match num {
            0 => writer.write_all(&replace_tabs(self.id.as_bytes()))?,
            1 => writer.write_all(self.sequence.as_ref())?,
            _ => panic!("FASTA field index out of range"),
        };
        Ok(())
    }
}

pub struct FastaRecordT;
impl<'b> BindT<'b> for FastaRecordT {
    type Assoc = FastaRecord<'b>;
}

pub struct FastaReaderBuilder;

impl Default for FastaReaderBuilder {
    fn default() -> Self {
        FastaReaderBuilder
    }
}

impl ReaderBuilder for FastaReaderBuilder {
    type Item = FastaRecordT;

    fn to_reader<'r>(
        &self,
        rb: ReadBuffer<'r>,
    ) -> Result<Box<dyn RecordReader<Item = Self::Item> + 'r>, EtError> {
        Ok(Box::new(FastaReader { rb }))
    }
}

pub struct FastaReader<'r> {
    rb: ReadBuffer<'r>,
}

impl<'r> RecordReader for FastaReader<'r> {
    type Item = FastaRecordT;

    fn headers(&self) -> Vec<&str> {
        vec!["id", "sequence"]
    }

    fn next(&mut self) -> Result<Option<FastaRecord>, EtError> {
        if self.rb.is_empty() {
            return Ok(None);
        }
        if self.rb[0] != b'>' {
            return Err(EtError::new("Valid FASTA records start with '>'").fill_pos(&self.rb));
        }
        let (header_range, seq_range, rec_end) = loop {
            let (header_end, seq_start) = if let Some(p) = memchr(b'\n', &self.rb[..]) {
                (p, p + 1)
            } else if self.rb.eof() {
                return Err(EtError::new("Incomplete record").fill_pos(&self.rb));
            } else {
                self.rb.refill()?;
                continue;
            };
            let (seq_end, rec_end) = if let Some(p) = memchr(b'>', &self.rb[seq_start..]) {
                // there must be a newline right before the >
                // (we're only looking for the > because it's
                // faster than looking at all the newlines)
                if self.rb[seq_start + p - 1] != b'\n' {
                    return Err(EtError::new("Unexpected > found in sequence").fill_pos(&self.rb));
                }
                // the > is technically part of the next record so we short by one
                (seq_start + p - 1, seq_start + p)
            } else if self.rb.eof() {
                // we're at the end so just return the rest of the sequence
                (self.rb.len(), self.rb.len())
            } else {
                self.rb.refill()?;
                continue;
            };
            break (1..header_end, seq_start..seq_end, rec_end);
        };

        let record = self.rb.consume(rec_end);

        let header = &record[header_range];
        let seq = &record[seq_range];

        Ok(Some(FastaRecord {
            id: std::str::from_utf8(header)?,
            sequence: seq.into(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::ReadBuffer;
    use std::borrow::Cow;
    use std::io::Cursor;

    #[test]
    fn test_fasta_reading() -> Result<(), EtError> {
        const TEST_FASTA: &str = ">id\nACGT\n>id2\nTGCA";
        let rb = ReadBuffer::with_capacity(5, Box::new(Cursor::new(TEST_FASTA)))?;
        let mut pt = FastaReaderBuilder::default().to_reader(rb)?;
        assert_eq!(pt.headers(), vec!["id", "sequence"]);

        let mut ix = 0;
        while let Some(l) = pt.next()? {
            match ix {
                0 => {
                    assert_eq!(l.id, "id");
                    assert_eq!(l.sequence, Cow::Borrowed(&b"ACGT"[..]));
                }
                1 => {
                    assert_eq!(l.id, "id2");
                    assert_eq!(l.sequence, Cow::Borrowed(&b"TGCA"[..]));
                }
                _ => return Err("bad line".into()),
            }
            ix += 1;
        }
        assert_eq!(ix, 2);
        Ok(())
    }
}
