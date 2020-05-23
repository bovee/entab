use memchr::memchr;

use crate::buffer::ReadBuffer;
use crate::record::{BindT, ReaderBuilder, Record, RecordReader};
use crate::utils::string::replace_tabs;
use crate::EtError;

#[derive(Debug)]
pub struct FastqRecord<'s> {
    pub id: &'s str,
    pub sequence: &'s [u8],
    pub quality: &'s [u8],
}

impl<'s> Record for FastqRecord<'s> {
    fn size(&self) -> usize {
        3
    }

    fn write_field<W>(&self, index: usize, mut write: W) -> Result<(), EtError>
    where
        W: FnMut(&[u8]) -> Result<(), EtError>,
    {
        match index {
            0 => write(&replace_tabs(self.id.as_bytes(), b'|'))?,
            1 => write(self.sequence)?,
            2 => write(self.quality)?,
            _ => panic!("FASTQ field index out of range"),
        };
        Ok(())
    }
}

pub struct FastqRecordT;
impl<'b> BindT<'b> for FastqRecordT {
    type Assoc = FastqRecord<'b>;
}

pub struct FastqReaderBuilder;

impl Default for FastqReaderBuilder {
    fn default() -> Self {
        FastqReaderBuilder
    }
}

impl ReaderBuilder for FastqReaderBuilder {
    type Item = FastqRecordT;

    fn to_reader<'r>(
        &self,
        rb: ReadBuffer<'r>,
    ) -> Result<Box<dyn RecordReader<Item = Self::Item> + 'r>, EtError> {
        Ok(Box::new(FastqReader { rb }))
    }
}

pub struct FastqReader<'r> {
    rb: ReadBuffer<'r>,
}

impl<'r> RecordReader for FastqReader<'r> {
    type Item = FastqRecordT;

    fn headers(&self) -> Vec<&str> {
        vec!["id", "sequence", "quality"]
    }

    fn next(&mut self) -> Result<Option<FastqRecord>, EtError> {
        if self.rb.is_empty() {
            if self.rb.eof() {
                return Ok(None);
            }
            self.rb.refill()?;
        }
        if self.rb[0] != b'@' {
            return Err("Valid FASTQ records start with '@'".into());
        }
        let (header_range, seq_range, qual_range, rec_end) = loop {
            let (header_end, seq_start) = if let Some(p) = memchr(b'\n', &self.rb[..]) {
                if p > 0 && self.rb[p - 1] == b'\r' {
                    // strip out the \r too if this is a \r\n ending
                    (p - 1, p + 1)
                } else {
                    (p, p + 1)
                }
            } else if self.rb.eof() {
                return Err("Record ended prematurely in header".into());
            } else {
                self.rb.refill()?;
                continue;
            };
            let (seq_end, id2_start) = if let Some(p) = memchr(b'+', &self.rb[seq_start..]) {
                if p == 0 || self.rb[seq_start + p - 1] != b'\n' {
                    return Err("Unexpected + found in sequence".into());
                }
                // the + is technically part of the next header so we're
                // already one short before we even check the \r
                if seq_start + p > 2 && self.rb[seq_start + p - 2] == b'\r' {
                    // strip out the \r too if this is a \r\n ending
                    (seq_start + p - 2, seq_start + p)
                } else {
                    (seq_start + p - 1, seq_start + p)
                }
            } else if self.rb.eof() {
                return Err("Record ended prematurely in sequence".into());
            } else {
                self.rb.refill()?;
                continue;
            };
            let qual_start = if let Some(p) = memchr(b'\n', &self.rb[id2_start..]) {
                id2_start + p + 1
            } else if self.rb.eof() {
                return Err("Record ended prematurely in second header".into());
            } else {
                self.rb.refill()?;
                continue;
            };

            let qual_end = qual_start + (seq_end - seq_start);
            let mut rec_end = qual_end + (id2_start - seq_end);
            // sometimes the terminal one or two newlines might be missing
            // so we deduct here to avoid a error overconsuming
            if rec_end > self.rb.len() && self.rb.eof() {
                rec_end -= id2_start - seq_end;
            }

            if qual_end > self.rb.len() {
                return Err("Record ended prematurely in quality".into());
            } else if rec_end > self.rb.len() && !self.rb.eof() {
                self.rb.refill()?;
                continue;
            }

            break (
                1..header_end,
                seq_start..seq_end,
                qual_start..qual_end,
                rec_end,
            );
        };

        let record = self.rb.consume(rec_end);

        Ok(Some(FastqRecord {
            id: std::str::from_utf8(&record[header_range])?,
            sequence: &record[seq_range],
            quality: &record[qual_range],
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
        assert_eq!(pt.headers(), vec!["id", "sequence", "quality"]);

        let l = pt.next()?.expect("first record present");
        assert_eq!(l.id, "id");
        assert_eq!(l.sequence, &b"ACGT"[..]);
        assert_eq!(l.quality, &b"!!!!"[..]);

        let l = pt.next()?.expect("first record present");
        assert_eq!(l.id, "id2");
        assert_eq!(l.sequence, &b"TGCA"[..]);
        assert_eq!(l.quality, &b"!!!!"[..]);

        assert!(pt.next()?.is_none());
        Ok(())
    }

    #[test]
    fn test_fastq_extra_newlines() -> Result<(), EtError> {
        const TEST_FASTQ: &[u8] = b"@id\r\nACGT\r\n+\r\n!!!!\r\n@id2\r\nTGCA\r\n+\r\n!!!!\r\n";
        let rb = ReadBuffer::from_slice(TEST_FASTQ);
        let mut pt = FastqReaderBuilder::default().to_reader(rb)?;
        assert_eq!(pt.headers(), vec!["id", "sequence", "quality"]);

        let l = pt.next()?.expect("first record present");
        assert_eq!(l.id, "id");
        assert_eq!(l.sequence, &b"ACGT"[..]);
        assert_eq!(l.quality, &b"!!!!"[..]);

        let l = pt.next()?.expect("first record present");
        assert_eq!(l.id, "id2");
        assert_eq!(l.sequence, &b"TGCA"[..]);
        assert_eq!(l.quality, &b"!!!!"[..]);

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
}
