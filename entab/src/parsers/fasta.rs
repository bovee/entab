use alloc::vec;
use alloc::vec::Vec;

use memchr::{memchr, memchr_iter};

use crate::parsers::FromSlice;
use crate::record::StateMetadata;
use crate::EtError;
use crate::{impl_reader, impl_record};

use alloc::borrow::Cow;

#[derive(Clone, Debug, Default)]
/// A single sequence from a FASTA file
pub struct FastaRecord<'r> {
    /// The ID/header line
    pub id: &'r str,
    /// The sequence itself
    pub sequence: Cow<'r, [u8]>,
}

impl_record!(FastaRecord<'r>: id, sequence);

/// The current state of FASTA parsing
#[derive(Clone, Copy, Debug, Default)]
pub struct FastaState {
    header_end: usize,
    seq: (usize, usize),
}

impl StateMetadata for FastaState {
    fn header(&self) -> Vec<&str> {
        vec!["id", "sequence"]
    }
}

impl<'b: 's, 's> FromSlice<'b, 's> for FastaState {
    type State = ();
}

impl<'b: 's, 's> FromSlice<'b, 's> for FastaRecord<'b> {
    type State = FastaState;

    fn parse(
        rb: &[u8],
        eof: bool,
        consumed: &mut usize,
        parser_state: &mut Self::State,
    ) -> Result<bool, EtError> {
        if !eof && rb.is_empty() {
            // TODO: also check if it's just some whitespace?
            return Err(EtError::new("No FASTA could be parsed").incomplete());
        } else if eof && rb.is_empty() {
            return Ok(false);
        }
        if rb[0] != b'>' {
            return Err("Valid FASTA records start with '>'".into());
        }
        let seq_start = if let Some(p) = memchr(b'\n', rb) {
            if p > 0 && rb[p - 1] == b'\r' {
                // strip out the \r too if this is a \r\n ending
                parser_state.header_end = p - 1;
                p + 1
            } else {
                parser_state.header_end = p;
                p + 1
            }
        } else {
            return Err(EtError::new("Incomplete header").incomplete());
        };

        if let Some(p) = memchr(b'>', &rb[seq_start..]) {
            if p == 0 || rb.get(seq_start + p - 1) != Some(&b'\n') {
                return Err("Unexpected '>' found".into());
            }
            if rb.get(seq_start + p - 2) == Some(&b'\r') {
                parser_state.seq = (seq_start, seq_start + p - 2);
            } else {
                parser_state.seq = (seq_start, seq_start + p - 1);
            }
            *consumed += seq_start + p;
        } else if eof {
            parser_state.seq = (seq_start, rb.len());
            // at eof; just return the end
            *consumed += rb.len();
        } else {
            return Err(EtError::new("Sequence needs more data").incomplete());
        }
        Ok(true)
    }

    fn get(&mut self, rb: &'b [u8], state: &Self::State) -> Result<(), EtError> {
        self.id = alloc::str::from_utf8(&rb[1..state.header_end])?;
        let raw_sequence = &rb[state.seq.0..state.seq.1];
        let mut seq_newlines = memchr_iter(b'\n', raw_sequence).peekable();
        self.sequence = if seq_newlines.peek().is_none() {
            raw_sequence.into()
        } else {
            let mut new_buf = Vec::with_capacity(raw_sequence.len());
            let mut start = 0;
            for pos in seq_newlines {
                if pos >= 1 && raw_sequence.get(pos - 1) == Some(&b'\r') {
                    new_buf.extend_from_slice(&raw_sequence[start..pos - 1]);
                } else {
                    new_buf.extend_from_slice(&raw_sequence[start..pos]);
                }
                start = pos + 1;
            }
            new_buf.extend_from_slice(&raw_sequence[start..]);
            new_buf.into()
        };
        Ok(())
    }
}

impl_reader!(FastaReader, FastaRecord, FastaRecord<'r>, FastaState, ());

#[cfg(test)]
mod tests {
    use alloc::borrow::Cow;

    use super::*;

    #[test]
    fn test_fasta_reading() -> Result<(), EtError> {
        const TEST_FASTA: &[u8] = b">id\nACGT\n>id2\nTGCA";
        let mut pt = FastaReader::new(TEST_FASTA, None)?;

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
        let mut pt = FastaReader::new(TEST_FASTA, None)?;
        assert!(pt.next().is_err());

        const TEST_FASTA_2: &[u8] = b">\n>";
        let mut pt = FastaReader::new(TEST_FASTA_2, None)?;
        assert!(pt.next().is_err());

        Ok(())
    }

    #[test]
    fn test_fasta_multiline() -> Result<(), EtError> {
        const TEST_FASTA: &[u8] = b">id\nACGT\nAAAA\n>id2\nTGCA";
        let mut pt = FastaReader::new(TEST_FASTA, None)?;

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
        let mut pt = FastaReader::new(TEST_FASTA, None)?;

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
        let mut pt = FastaReader::new(TEST_FASTA, None)?;

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
