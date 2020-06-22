use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;

use memchr::{memchr, memchr_iter};

use crate::buffer::ReadBuffer;
use crate::record::{ReaderBuilder, Record, RecordReader};
use crate::EtError;
pub struct FastaReaderBuilder;

impl Default for FastaReaderBuilder {
    fn default() -> Self {
        FastaReaderBuilder
    }
}

impl ReaderBuilder for FastaReaderBuilder {
    fn to_reader<'r>(&self, rb: ReadBuffer<'r>) -> Result<Box<dyn RecordReader + 'r>, EtError> {
        Ok(Box::new(FastaReader { rb }))
    }
}

pub struct FastaReader<'r> {
    rb: ReadBuffer<'r>,
}

impl<'r> RecordReader for FastaReader<'r> {
    fn headers(&self) -> Vec<&str> {
        vec!["id", "sequence"]
    }

    fn next(&mut self) -> Result<Option<Record>, EtError> {
        if self.rb.is_empty() {
            return Ok(None);
        }
        if self.rb[0] != b'>' {
            return Err(EtError::new("Valid FASTA records start with '>'").fill_pos(&self.rb));
        }
        let mut seq_newlines: Vec<usize> = Vec::new();
        let (header_range, seq_range, rec_end) = loop {
            let (header_end, seq_start) = if let Some(p) = memchr(b'\n', &self.rb[..]) {
                if p > 0 && self.rb[p - 1] == b'\r' {
                    // strip out the \r too if this is a \r\n ending
                    (p - 1, p + 1)
                } else {
                    (p, p + 1)
                }
            } else if self.rb.eof() {
                return Err(EtError::new("Incomplete record").fill_pos(&self.rb));
            } else {
                self.rb.refill()?;
                continue;
            };
            let mut found_end = false;
            for raw_pos in memchr_iter(b'\n', &self.rb[seq_start..]) {
                let pos = seq_start + raw_pos;
                if pos > 0 && self.rb[pos - 1] == b'\r' {
                    seq_newlines.push(raw_pos - 1);
                }
                seq_newlines.push(raw_pos);
                if pos + 1 < self.rb.len() && self.rb[pos + 1] == b'>' {
                    found_end = true;
                    break;
                }
            }
            if !found_end && !self.rb.eof() {
                self.rb.refill()?;
                seq_newlines.truncate(0);
                continue;
            }
            let (seq_end, rec_end) = if found_end {
                // found_end only happens if we added a newline
                // so the pop is safe to unwrap
                let mut endpos = seq_newlines.pop().unwrap();
                let rec_end = seq_start + endpos + 1;

                // remove trailing consecutive newlines (e.g. \r\n)
                // from the end
                while endpos > 0 && seq_newlines.last() == Some(endpos - 1).as_ref() {
                    endpos = seq_newlines.pop().unwrap();
                }
                (seq_start + endpos, rec_end)
            } else {
                // at eof; just return the end
                (self.rb.len(), self.rb.len())
            };
            break (1..header_end, seq_start..seq_end, rec_end);
        };

        let record = self.rb.consume(rec_end);

        let header = &record[header_range];
        let raw_sequence = &record[seq_range];
        let sequence = if seq_newlines.is_empty() {
            raw_sequence.into()
        } else {
            let mut new_buf = Vec::with_capacity(raw_sequence.len() - seq_newlines.len());
            let mut start = 0;
            for pos in seq_newlines {
                new_buf.extend_from_slice(&raw_sequence[start..pos]);
                start = pos + 1;
            }
            new_buf.extend_from_slice(&raw_sequence[start..]);
            new_buf.into()
        };

        Ok(Some(Record::Fasta {
            id: alloc::str::from_utf8(header)?,
            sequence,
        }))
    }
}

#[cfg(test)]
mod tests {
    use alloc::borrow::Cow;

    use super::*;
    use crate::buffer::ReadBuffer;

    #[test]
    fn test_fasta_reading() -> Result<(), EtError> {
        const TEST_FASTA: &[u8] = b">id\nACGT\n>id2\nTGCA";
        let rb = ReadBuffer::from_slice(TEST_FASTA);
        let mut pt = FastaReaderBuilder::default().to_reader(rb)?;
        assert_eq!(pt.headers(), vec!["id", "sequence"]);

        let mut ix = 0;
        while let Some(Record::Fasta { id, sequence }) = pt.next()? {
            match ix {
                0 => {
                    assert_eq!(id, "id");
                    assert_eq!(sequence, Cow::Borrowed(&b"ACGT"[..]));
                }
                1 => {
                    assert_eq!(id, "id2");
                    assert_eq!(sequence, Cow::Borrowed(&b"TGCA"[..]));
                }
                _ => return Err("bad line".into()),
            }
            ix += 1;
        }
        assert_eq!(ix, 2);
        Ok(())
    }

    #[test]
    fn test_fasta_multiline() -> Result<(), EtError> {
        const TEST_FASTA: &[u8] = b">id\nACGT\nAAAA\n>id2\nTGCA";
        let rb = ReadBuffer::from_slice(TEST_FASTA);
        let mut pt = FastaReaderBuilder::default().to_reader(rb)?;
        assert_eq!(pt.headers(), vec!["id", "sequence"]);

        if let Record::Fasta { id, sequence } = pt.next()?.expect("first record present") {
            assert_eq!(id, "id");
            assert_eq!(sequence, Cow::Owned::<[u8]>(b"ACGTAAAA".to_vec()));
        } else {
            panic!("FASTA reader returned non-FASTA record");
        }

        if let Record::Fasta { id, sequence } = pt.next()?.expect("second record present") {
            assert_eq!(id, "id2");
            assert_eq!(sequence, Cow::Borrowed(b"TGCA"));
        } else {
            panic!("FASTA reader returned non-FASTA record");
        }

        assert!(pt.next()?.is_none());
        Ok(())
    }

    #[test]
    fn test_fasta_multiline_extra_newlines() -> Result<(), EtError> {
        const TEST_FASTA: &[u8] = b">id\r\nACGT\r\nAAAA\r\n>id2\r\nTGCA\r\n";
        let rb = ReadBuffer::from_slice(TEST_FASTA);
        let mut pt = FastaReaderBuilder::default().to_reader(rb)?;
        assert_eq!(pt.headers(), vec!["id", "sequence"]);

        if let Record::Fasta { id, sequence } = pt.next()?.expect("first record present") {
            assert_eq!(id, "id");
            assert_eq!(sequence, Cow::Owned::<[u8]>(b"ACGTAAAA".to_vec()));
        } else {
            panic!("FASTA reader returned non-FASTA record");
        }

        if let Record::Fasta { id, sequence } = pt.next()?.expect("second record present") {
            assert_eq!(id, "id2");
            assert_eq!(sequence, Cow::Borrowed(b"TGCA"));
        } else {
            panic!("FASTA reader returned non-FASTA record");
        }

        assert!(pt.next()?.is_none());
        Ok(())
    }

    #[test]
    fn test_fasta_empty_fields() -> Result<(), EtError> {
        const TEST_FASTA: &[u8] = b">hd\n\n>\n\n";
        let rb = ReadBuffer::from_slice(TEST_FASTA);
        let mut pt = FastaReaderBuilder::default().to_reader(rb)?;
        assert_eq!(pt.headers(), vec!["id", "sequence"]);

        if let Record::Fasta { id, sequence } = pt.next()?.expect("first record present") {
            assert_eq!(id, "hd");
            assert_eq!(sequence, Cow::Borrowed(b""));
        } else {
            panic!("FASTA reader returned non-FASTA record");
        }

        if let Record::Fasta { id, sequence } = pt.next()?.expect("second record present") {
            assert_eq!(id, "");
            assert_eq!(sequence, Cow::Borrowed(b""));
        } else {
            panic!("FASTA reader returned non-FASTA record");
        }

        assert!(pt.next()?.is_none());
        Ok(())
    }
}
