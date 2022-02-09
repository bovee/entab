use memchr::memchr;

use crate::parsers::FromSlice;
use crate::record::StateMetadata;
use crate::EtError;
use crate::{impl_reader, impl_record};

#[derive(Debug, Default)]
/// A single sequence with quality data from a FASTQ file
pub struct FastqRecord<'r> {
    /// The ID/header line
    pub id: &'r str,
    /// The sequence itself
    pub sequence: &'r [u8],
    /// The matching quality scores for bases in the sequence
    pub quality: &'r [u8],
}

impl_record!(FastqRecord<'r>: id, sequence, quality);

/// The current state of FASTQ parsing; note that we use tuples of usize because Range doesn't
/// support copying and tuples with an inclusive and exclusive bound are actually fairly slow.
#[derive(Clone, Copy, Debug, Default)]
pub struct FastqState {
    header_end: usize,
    seq: (usize, usize),
    qual: (usize, usize),
}

impl<'r> StateMetadata<'r> for FastqState {}

impl<'r> FromSlice<'r> for FastqState {
    type State = ();
}

impl<'r> FromSlice<'r> for FastqRecord<'r> {
    type State = &'r mut FastqState;

    fn parse(
        rb: &[u8],
        eof: bool,
        consumed: &mut usize,
        state: &mut Self::State,
    ) -> Result<bool, EtError> {
        if rb.is_empty() {
            if eof {
                return Ok(false);
            }
            return Err(EtError::new("No FASTQ could be parsed").incomplete());
        }
        if rb[0] != b'@' {
            return Err("Valid FASTQ records start with '@'".into());
        }
        // figure out where the first id/header line ends
        let seq_start = if let Some(p) = memchr(b'\n', rb) {
            if p > 0 && rb[p - 1] == b'\r' {
                // strip out the \r too if this is a \r\n ending
                state.header_end = p - 1;
            } else {
                state.header_end = p;
            }
            p + 1
        } else {
            return Err(EtError::new("Record ended prematurely in header").incomplete());
        };
        // figure out where the sequence data is
        let id2_start = if let Some(p) = memchr(b'+', &rb[seq_start..]) {
            if p == 0 || rb[seq_start + p - 1] != b'\n' {
                return Err("Unexpected + found in sequence".into());
            }
            // the + is technically part of the next header so we're
            // already one short before we even check the \r
            if seq_start + p > 2 && rb[seq_start + p - 2] == b'\r' {
                // strip out the \r too if this is a \r\n ending
                state.seq = (seq_start, seq_start + p - 2);
            } else {
                state.seq = (seq_start, seq_start + p - 1);
            }
            seq_start + p
        } else {
            return Err(EtError::new("Record ended prematurely in sequence").incomplete());
        };
        // skip over the second id/header line
        let qual_start = if let Some(p) = memchr(b'\n', &rb[id2_start..]) {
            id2_start + p + 1
        } else {
            return Err(EtError::new("Record ended prematurely in second header").incomplete());
        };
        // and get the quality scores location
        let qual_end = qual_start + (state.seq.1 - state.seq.0);
        let mut rec_end = qual_end + (id2_start - state.seq.1);
        // sometimes the terminal one or two newlines might be missing
        // so we deduct here to avoid a error overconsuming
        if rec_end > rb.len() && eof {
            rec_end -= id2_start - state.seq.1;
        }
        if rec_end > rb.len() {
            return Err(EtError::new("Record ended prematurely in quality").incomplete());
        }
        state.qual = (qual_start, qual_end);

        *consumed += rec_end;
        Ok(true)
    }

    fn get(&mut self, rb: &'r [u8], state: &Self::State) -> Result<(), EtError> {
        self.id = alloc::str::from_utf8(&rb[1..state.header_end])?;
        self.sequence = &rb[state.seq.0..state.seq.1];
        self.quality = &rb[state.qual.0..state.qual.1];
        Ok(())
    }
}

impl_reader!(FastqReader, FastqRecord, FastqState, ());

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fastq_reading() -> Result<(), EtError> {
        const TEST_FASTQ: &[u8] = b"@id\nACGT\n+\n!!!!\n@id2\nTGCA\n+\n!!!!";
        let mut pt = FastqReader::new(TEST_FASTQ, ())?;

        if let Some(FastqRecord {
            id,
            sequence,
            quality,
        }) = pt.next()?
        {
            assert_eq!(id, "id");
            assert_eq!(sequence, &b"ACGT"[..]);
            assert_eq!(quality, &b"!!!!"[..]);
        } else {
            panic!("FASTQ reader returned non-FASTQ reader");
        }

        if let Some(FastqRecord {
            id,
            sequence,
            quality,
        }) = pt.next()?
        {
            assert_eq!(id, "id2");
            assert_eq!(sequence, &b"TGCA"[..]);
            assert_eq!(quality, &b"!!!!"[..]);
        } else {
            panic!("FASTQ reader returned non-FASTQ reader");
        }

        assert!(pt.next()?.is_none());
        Ok(())
    }

    #[test]
    fn test_fastq_extra_newlines() -> Result<(), EtError> {
        const TEST_FASTQ: &[u8] = b"@id\r\nACGT\r\n+\r\n!!!!\r\n@id2\r\nTGCA\r\n+\r\n!!!!\r\n";
        let mut pt = FastqReader::new(TEST_FASTQ, ())?;

        if let Some(FastqRecord {
            id,
            sequence,
            quality,
        }) = pt.next()?
        {
            assert_eq!(id, "id");
            assert_eq!(sequence, &b"ACGT"[..]);
            assert_eq!(quality, &b"!!!!"[..]);
        } else {
            panic!("FASTQ reader returned non-FASTQ reader");
        }

        if let Some(FastqRecord {
            id,
            sequence,
            quality,
        }) = pt.next()?
        {
            assert_eq!(id, "id2");
            assert_eq!(sequence, &b"TGCA"[..]);
            assert_eq!(quality, &b"!!!!"[..]);
        } else {
            panic!("FASTQ reader returned non-FASTQ reader");
        }

        assert!(pt.next()?.is_none());
        Ok(())
    }

    #[test]
    fn test_fastq_pathological_sequences() -> Result<(), EtError> {
        const TEST_FASTQ_1: &[u8] = b"@DF\n+\n+\n!";
        let mut pt = FastqReader::new(TEST_FASTQ_1, ())?;
        assert!(pt.next().is_err());

        const TEST_FASTQ_2: &[u8] = b"@\n";
        let mut pt = FastqReader::new(TEST_FASTQ_2, ())?;
        assert!(pt.next().is_err());

        Ok(())
    }

    #[test]
    fn test_fastq_from_file() -> Result<(), EtError> {
        let data: &[u8] = include_bytes!("../../tests/data/test.fastq");
        let mut reader = FastqReader::new(data, ())?;
        while reader.next()?.is_some() {}
        Ok(())
    }
}
