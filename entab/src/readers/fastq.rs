use memchr::memchr;

use crate::buffer::ReadBuffer;
use crate::parsers::FromBuffer;
use crate::EtError;
use crate::{impl_reader, impl_record};

pub struct FastqRecord<'r> {
    pub id: &'r str,
    pub sequence: &'r [u8],
    pub quality: &'r [u8],
}

impl_record!(FastqRecord<'r>: id, sequence, quality);

impl<'r> FromBuffer<'r> for Option<FastqRecord<'r>> {
    type State = &'r mut ();

    fn get(rb: &'r mut ReadBuffer, _state: Self::State) -> Result<Self, EtError> {
        if rb.is_empty() {
            if rb.eof() {
                return Ok(None);
            }
            rb.refill()?;
            // if the buffer perfectly aligns, it's possible we could do a zero-byte read
            // and now we're in an EOF
            if rb.eof() {
                return Ok(None);
            }
        }
        if rb[0] != b'@' {
            return Err("Valid FASTQ records start with '@'".into());
        }
        let (header_range, seq_range, qual_range, rec_end) = loop {
            let (header_end, seq_start) = if let Some(p) = memchr(b'\n', &rb[..]) {
                if p > 0 && rb[p - 1] == b'\r' {
                    // strip out the \r too if this is a \r\n ending
                    (p - 1, p + 1)
                } else {
                    (p, p + 1)
                }
            } else if rb.eof() {
                return Err("Record ended prematurely in header".into());
            } else {
                rb.refill()?;
                continue;
            };
            let (seq_end, id2_start) = if let Some(p) = memchr(b'+', &rb[seq_start..]) {
                if p == 0 || rb[seq_start + p - 1] != b'\n' {
                    return Err("Unexpected + found in sequence".into());
                }
                // the + is technically part of the next header so we're
                // already one short before we even check the \r
                if seq_start + p > 2 && rb[seq_start + p - 2] == b'\r' {
                    // strip out the \r too if this is a \r\n ending
                    (seq_start + p - 2, seq_start + p)
                } else {
                    (seq_start + p - 1, seq_start + p)
                }
            } else if rb.eof() {
                return Err("Record ended prematurely in sequence".into());
            } else {
                rb.refill()?;
                continue;
            };
            let qual_start = if let Some(p) = memchr(b'\n', &rb[id2_start..]) {
                id2_start + p + 1
            } else if rb.eof() {
                return Err("Record ended prematurely in second header".into());
            } else {
                rb.refill()?;
                continue;
            };

            let qual_end = qual_start + (seq_end - seq_start);
            let mut rec_end = qual_end + (id2_start - seq_end);
            // sometimes the terminal one or two newlines might be missing
            // so we deduct here to avoid a error overconsuming
            if rec_end > rb.len() && rb.eof() {
                rec_end -= id2_start - seq_end;
            }

            if qual_end > rb.len() && rb.eof() {
                return Err("Record ended prematurely in quality".into());
            } else if rec_end > rb.len() && !rb.eof() {
                rb.refill()?;
                continue;
            }

            break (
                1..header_end,
                seq_start..seq_end,
                qual_start..qual_end,
                rec_end,
            );
        };

        let record = rb.consume(rec_end);

        Ok(Some(FastqRecord {
            id: alloc::str::from_utf8(&record[header_range])?,
            sequence: &record[seq_range],
            quality: &record[qual_range],
        }))
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

    #[cfg(feature = "std")]
    #[test]
    fn test_fastq_from_file() -> Result<(), EtError> {
        use std::fs::File;

        let f = File::open("tests/data/test.fastq")?;
        let rb = ReadBuffer::new(Box::new(&f))?;
        let mut reader = FastqReader::new(rb, ())?;
        while let Some(_) = reader.next()? {}
        Ok(())
    }
}
