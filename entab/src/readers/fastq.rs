use memchr::memchr;

use crate::buffer::ReadBuffer;
use crate::parsers::FromBuffer;
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

impl<'r> FromBuffer<'r> for FastqRecord<'r> {
    type State = &'r mut ();

    fn from_buffer(
        &mut self,
        rb: &'r mut ReadBuffer,
        _state: Self::State,
    ) -> Result<bool, EtError> {
        if rb.is_empty() {
            if rb.eof() {
                return Ok(false);
            }
            rb.refill()?;
            // if the buffer perfectly aligns, it's possible we could do a zero-byte read
            // and now we're in an EOF
            if rb.eof() {
                return Ok(false);
            }
        }
        if rb[0] != b'@' {
            return Err(EtError::new("Valid FASTQ records start with '@'", &rb));
        }
        // figure out where the first id/header line ends
        let (header_range, seq_start) = loop {
            if let Some(p) = memchr(b'\n', &rb[..]) {
                if p > 0 && rb[p - 1] == b'\r' {
                    // strip out the \r too if this is a \r\n ending
                    break (1..p - 1, p + 1);
                } else {
                    break (1..p, p + 1);
                }
            } else if rb.eof() {
                return Err(EtError::new("Record ended prematurely in header", &rb));
            }
            rb.refill()?;
        };
        // figure out where the sequence data is
        let (seq_range, id2_start) = loop {
            if let Some(p) = memchr(b'+', &rb[seq_start..]) {
                if p == 0 || rb[seq_start + p - 1] != b'\n' {
                    return Err(EtError::new("Unexpected + found in sequence", &rb));
                }
                // the + is technically part of the next header so we're
                // already one short before we even check the \r
                if seq_start + p > 2 && rb[seq_start + p - 2] == b'\r' {
                    // strip out the \r too if this is a \r\n ending
                    break (seq_start..seq_start + p - 2, seq_start + p);
                } else {
                    break (seq_start..seq_start + p - 1, seq_start + p);
                }
            } else if rb.eof() {
                return Err(EtError::new("Record ended prematurely in sequence", &rb));
            }
            rb.refill()?;
        };
        // skip over the second id/header line
        let qual_start = loop {
            if let Some(p) = memchr(b'\n', &rb[id2_start..]) {
                break id2_start + p + 1;
            } else if rb.eof() {
                return Err(EtError::new(
                    "Record ended prematurely in second header",
                    &rb,
                ));
            }
            rb.refill()?;
        };
        // and get the quality scores location
        let (qual_range, rec_end) = loop {
            let qual_end = qual_start + (seq_range.end - seq_range.start);
            let mut rec_end = qual_end + (id2_start - seq_range.end);
            // sometimes the terminal one or two newlines might be missing
            // so we deduct here to avoid a error overconsuming
            if rec_end > rb.len() && rb.eof() {
                rec_end -= id2_start - seq_range.end;
            }

            if qual_end > rb.len() && rb.eof() {
                return Err(EtError::new("Record ended prematurely in quality", &rb));
            } else if rec_end > rb.len() && !rb.eof() {
                rb.refill()?;
                continue;
            }

            break (qual_start..qual_end, rec_end);
        };

        let record = rb.extract::<&[u8]>(rec_end)?;
        self.id = alloc::str::from_utf8(&record[header_range])?;
        self.sequence = &record[seq_range];
        self.quality = &record[qual_range];
        Ok(true)
    }
}

impl_reader!(FastqReader, FastqRecord, (), ());

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::ReadBuffer;

    #[test]
    fn test_fastq_reading() -> Result<(), EtError> {
        const TEST_FASTQ: &[u8] = b"@id\nACGT\n+\n!!!!\n@id2\nTGCA\n+\n!!!!";
        let rb = ReadBuffer::from_slice(TEST_FASTQ);
        let mut pt = FastqReader::new(rb, ())?;

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
        let rb = ReadBuffer::from_slice(TEST_FASTQ);
        let mut pt = FastqReader::new(rb, ())?;

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
        let rb = ReadBuffer::from_slice(TEST_FASTQ_1);
        let mut pt = FastqReader::new(rb, ())?;
        assert!(pt.next().is_err());

        const TEST_FASTQ_2: &[u8] = b"@\n";
        let rb = ReadBuffer::from_slice(TEST_FASTQ_2);
        let mut pt = FastqReader::new(rb, ())?;
        assert!(pt.next().is_err());

        Ok(())
    }

    #[test]
    fn test_fastq_from_file() -> Result<(), EtError> {
        let rb = ReadBuffer::from_slice(include_bytes!("../../tests/data/test.fastq"));
        let mut reader = FastqReader::new(rb, ())?;
        while let Some(_) = reader.next()? {}
        Ok(())
    }
}
