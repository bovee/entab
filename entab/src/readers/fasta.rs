use alloc::vec::Vec;

use memchr::{memchr, memchr_iter};

use crate::buffer::ReadBuffer;
use crate::parsers::FromBuffer;
use crate::record::StateMetadata;
use crate::EtError;
use crate::{impl_reader, impl_record};

use alloc::borrow::Cow;

#[derive(Debug, Default)]
/// A single sequence from a FASTA file
pub struct FastaRecord<'r> {
    /// The ID/header line
    pub id: &'r str,
    /// The sequence itself
    pub sequence: Cow<'r, [u8]>,
}

impl_record!(FastaRecord<'r>: id, sequence);

impl<'r> FromBuffer<'r> for FastaRecord<'r> {
    type State = &'r mut ();

    fn from_buffer(
        &mut self,
        rb: &'r mut ReadBuffer,
        _state: Self::State,
    ) -> Result<bool, EtError> {
        if rb.is_empty() {
            return Ok(false);
        }
        if rb[0] != b'>' {
            return Err(EtError::new("Valid FASTA records start with '>'", &rb));
        }
        let (header_range, seq_start) = loop {
            if let Some(p) = memchr(b'\n', &rb[..]) {
                if p > 0 && rb[p - 1] == b'\r' {
                    // strip out the \r too if this is a \r\n ending
                    break (1..p - 1, p + 1);
                } else {
                    break (1..p, p + 1);
                }
            } else if rb.eof() {
                return Err(EtError::new("Incomplete record", &rb));
            }
            rb.refill()?;
        };
        let mut seq_newlines: Vec<usize> = Vec::new();
        let (seq_range, rec_end) = loop {
            let mut found_end = false;
            for raw_pos in memchr_iter(b'\n', &rb[seq_start..]) {
                let pos = seq_start + raw_pos;
                if pos > 0 && rb[pos - 1] == b'\r' {
                    seq_newlines.push(raw_pos - 1);
                }
                seq_newlines.push(raw_pos);
                if pos + 1 < rb.len() && rb[pos + 1] == b'>' {
                    found_end = true;
                    break;
                }
            }
            if found_end {
                // found_end only happens if we added a newline
                // so the pop is safe to unwrap
                let mut endpos = seq_newlines.pop().unwrap();
                let rec_end = seq_start + endpos + 1;

                // remove trailing consecutive newlines (e.g. \r\n)
                // from the end
                while endpos > 0 && seq_newlines.last() == Some(endpos - 1).as_ref() {
                    endpos = seq_newlines.pop().unwrap();
                }
                break (seq_start..seq_start + endpos, rec_end);
            } else if rb.eof() {
                // at eof; just return the end
                break (seq_start..rb.len(), rb.len());
            };
            // need more data
            rb.refill()?;
            // TODO: we probably don't need to reset this if we track our
            // current position
            seq_newlines.truncate(0);
        };

        let record = rb.extract::<&[u8]>(rec_end)?;
        self.id = alloc::str::from_utf8(&record[header_range])?;
        let raw_sequence = &record[seq_range];
        self.sequence = if seq_newlines.is_empty() {
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

        Ok(true)
    }
}

impl_reader!(FastaReader, FastaRecord, (), ());

#[cfg(test)]
mod tests {
    use alloc::borrow::Cow;

    use super::*;
    use crate::buffer::ReadBuffer;

    #[test]
    fn test_fasta_reading() -> Result<(), EtError> {
        const TEST_FASTA: &[u8] = b">id\nACGT\n>id2\nTGCA";
        let rb = ReadBuffer::from_slice(TEST_FASTA);
        let mut pt = FastaReader::new(rb, ())?;

        let mut ix = 0;
        while let Some(FastaRecord { id, sequence }) = pt.next()? {
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
    fn test_fasta_short() -> Result<(), EtError> {
        const TEST_FASTA: &[u8] = b">id";
        let rb = ReadBuffer::from_slice(TEST_FASTA);
        let mut pt = FastaReader::new(rb, ())?;
        assert!(pt.next().is_err());

        Ok(())
    }

    #[test]
    fn test_fasta_multiline() -> Result<(), EtError> {
        const TEST_FASTA: &[u8] = b">id\nACGT\nAAAA\n>id2\nTGCA";
        let rb = ReadBuffer::from_slice(TEST_FASTA);
        let mut pt = FastaReader::new(rb, ())?;

        let FastaRecord { id, sequence } = pt.next()?.expect("first record present");
        assert_eq!(id, "id");
        assert_eq!(sequence, Cow::Owned::<[u8]>(b"ACGTAAAA".to_vec()));

        let FastaRecord { id, sequence } = pt.next()?.expect("second record present");
        assert_eq!(id, "id2");
        assert_eq!(sequence, Cow::Borrowed(b"TGCA"));

        assert!(pt.next()?.is_none());
        Ok(())
    }

    #[test]
    fn test_fasta_multiline_extra_newlines() -> Result<(), EtError> {
        const TEST_FASTA: &[u8] = b">id\r\nACGT\r\nAAAA\r\n>id2\r\nTGCA\r\n";
        let rb = ReadBuffer::from_slice(TEST_FASTA);
        let mut pt = FastaReader::new(rb, ())?;

        let FastaRecord { id, sequence } = pt.next()?.expect("first record present");
        assert_eq!(id, "id");
        assert_eq!(sequence, Cow::Owned::<[u8]>(b"ACGTAAAA".to_vec()));

        let FastaRecord { id, sequence } = pt.next()?.expect("second record present");
        assert_eq!(id, "id2");
        assert_eq!(sequence, Cow::Borrowed(b"TGCA"));

        assert!(pt.next()?.is_none());
        Ok(())
    }

    #[test]
    fn test_fasta_empty_fields() -> Result<(), EtError> {
        const TEST_FASTA: &[u8] = b">hd\n\n>\n\n";
        let rb = ReadBuffer::from_slice(TEST_FASTA);
        let mut pt = FastaReader::new(rb, ())?;

        let FastaRecord { id, sequence } = pt.next()?.expect("first record present");
        assert_eq!(id, "hd");
        assert_eq!(sequence, Cow::Borrowed(b""));

        let FastaRecord { id, sequence } = pt.next()?.expect("second record present");
        assert_eq!(id, "");
        assert_eq!(sequence, Cow::Borrowed(b""));

        assert!(pt.next()?.is_none());
        Ok(())
    }
}
