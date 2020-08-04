use alloc::boxed::Box;

use memchr::memchr;

use crate::buffer::ReadBuffer;
use crate::readers::{ReaderBuilder, RecordReader};
use crate::record::Record;
use crate::EtError;

pub struct FastqRecord<'r> {
    id: &'r str,
    sequence: &'r [u8],
    quality: &'r [u8]
}

use crate::buffer::FromBuffer;

impl<'r, 's> FromBuffer<'r, 's> for Option<FastqRecord<'r>> {
    type State = ();

    fn get(rb: &'r mut ReadBuffer<'s>, _amt: Self::State) -> Result<Self, EtError> {
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

pub struct FastqReaderBuilder;

impl Default for FastqReaderBuilder {
    fn default() -> Self {
        FastqReaderBuilder
    }
}

impl ReaderBuilder for FastqReaderBuilder {
    fn to_reader<'r>(&self, rb: ReadBuffer<'r>) -> Result<Box<dyn RecordReader + 'r>, EtError> {
        Ok(Box::new(FastqReader { rb }))
    }
}

pub struct FastqReader<'r> {
    pub rb: ReadBuffer<'r>,
}

impl<'r> RecordReader for FastqReader<'r> {
    fn next(&mut self) -> Result<Option<Record>, EtError> {
        Ok(self.rb.extract::<Option<FastqRecord>>(())?.map(|r: FastqRecord| {
            Record::Sequence {
                id: r.id,
                sequence: r.sequence.into(),
                quality: Some(r.quality),
            }
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::ReadBuffer;

    #[test]
    fn test_fastq_reading() -> Result<(), EtError> {
        const TEST_FASTQ: &[u8] = b"@id\nACGT\n+\n!!!!\n@id2\nTGCA\n+\n!!!!";
        let rb = ReadBuffer::from_slice(TEST_FASTQ);
        let mut pt = FastqReaderBuilder::default().to_reader(rb)?;

        if let Some(Record::Sequence {
            id,
            sequence,
            quality,
        }) = pt.next()?
        {
            assert_eq!(id, "id");
            assert_eq!(sequence, &b"ACGT"[..]);
            assert_eq!(quality, Some(&b"!!!!"[..]));
        } else {
            panic!("FASTQ reader returned non-FASTQ reader");
        }

        if let Some(Record::Sequence {
            id,
            sequence,
            quality,
        }) = pt.next()?
        {
            assert_eq!(id, "id2");
            assert_eq!(sequence, &b"TGCA"[..]);
            assert_eq!(quality, Some(&b"!!!!"[..]));
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
        let mut pt = FastqReaderBuilder::default().to_reader(rb)?;

        if let Some(Record::Sequence {
            id,
            sequence,
            quality,
        }) = pt.next()?
        {
            assert_eq!(id, "id");
            assert_eq!(sequence, &b"ACGT"[..]);
            assert_eq!(quality, Some(&b"!!!!"[..]));
        } else {
            panic!("FASTQ reader returned non-FASTQ reader");
        }

        if let Some(Record::Sequence {
            id,
            sequence,
            quality,
        }) = pt.next()?
        {
            assert_eq!(id, "id2");
            assert_eq!(sequence, &b"TGCA"[..]);
            assert_eq!(quality, Some(&b"!!!!"[..]));
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
        let mut pt = FastqReaderBuilder::default().to_reader(rb)?;
        assert!(pt.next().is_err());

        const TEST_FASTQ_2: &[u8] = b"@\n";
        let rb = ReadBuffer::from_slice(TEST_FASTQ_2);
        let mut pt = FastqReaderBuilder::default().to_reader(rb)?;
        assert!(pt.next().is_err());

        Ok(())
    }

    #[cfg(feature = "std")]
    #[test]
    fn test_fastq_from_file() -> Result<(), EtError> {
        use std::fs::File;

        let f = File::open("tests/data/test.fastq")?;
        let rb = ReadBuffer::new(Box::new(&f))?;
        let builder = FastqReaderBuilder::default();
        let mut reader = builder.to_reader(rb)?;
        while let Some(_) = reader.next()? {}
        Ok(())
    }
}
