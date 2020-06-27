use crate::buffer::ReadBuffer;
use crate::record::{ReaderBuilder, Record, RecordReader};
use crate::EtError;

pub struct SamReaderBuilder;

impl Default for SamReaderBuilder {
    fn default() -> Self {
        SamReaderBuilder
    }
}

impl ReaderBuilder for SamReaderBuilder {
    fn to_reader<'r>(&self, mut rb: ReadBuffer<'r>) -> Result<Box<dyn RecordReader + 'r>, EtError> {
        // eventually we should read the headers and pass them along
        // to the Reader as metadata once we support that
        while rb[0] == b'@' {
            if !rb.seek_pattern(b"\n")? {
                break;
            }
            // read the newline too
            rb.partial_consume(1);
        }

        let reader = SamReader { rb };
        Ok(Box::new(reader))
    }
}

pub struct SamReader<'r> {
    rb: ReadBuffer<'r>,
}

fn strs_to_sam<'r>(chunks: &[&'r [u8]]) -> Result<Record<'r>, EtError> {
    if chunks.len() < 12 {
        return Err("Sam record too short".into());
    }
    let pos = if chunks[3] == b"0" {
        None
    } else {
        // convert to 0-based indexing while we're at it
        let mut val = alloc::str::from_utf8(chunks[3])?.parse()?;
        val -= 1;
        Some(val)
    };
    let mapq = if chunks[4] == b"255" {
        None
    } else {
        Some(alloc::str::from_utf8(chunks[4])?.parse()?)
    };
    let rnext = if chunks[6] == b"*" {
        None
    } else {
        Some(alloc::str::from_utf8(chunks[6])?)
    };
    let pnext = if chunks[7] == b"0" {
        None
    } else {
        Some(alloc::str::from_utf8(chunks[7])?.parse()?)
    };
    let seq = if chunks[9] == b"*" {
        None
    } else {
        Some(chunks[9])
    };
    let qual = if chunks[10] == b"*" {
        None
    } else {
        Some(chunks[10])
    };
    Ok(Record::Sam {
        query_name: alloc::str::from_utf8(chunks[0])?,
        flag: alloc::str::from_utf8(chunks[1])?.parse()?,
        ref_name: alloc::str::from_utf8(chunks[2])?,
        pos,
        mapq,
        cigar: chunks[5],
        rnext,
        pnext,
        tlen: alloc::str::from_utf8(chunks[8])?.parse()?,
        seq,
        qual,
        extra: alloc::str::from_utf8(chunks[11])?,
    })
}

impl<'r> RecordReader for SamReader<'r> {
    fn headers(&self) -> Vec<&str> {
        // FIXME: need header names
        vec![]
    }

    fn next(&mut self) -> Result<Option<Record>, EtError> {
        let buffer_pos = (self.rb.reader_pos, self.rb.record_pos);
        Ok(if let Some(line) = self.rb.read_line()? {
            let chunks: Vec<&[u8]> = line.split(|c| *c == b'\t').collect();
            Some(strs_to_sam(&chunks).map_err(|mut e| {
                // we can't use `fill_pos` b/c that touchs the buffer
                // and messes up the lifetimes :/
                e.byte = Some(buffer_pos.0);
                e.record = Some(buffer_pos.1 + 1);
                e
            })?)
        } else {
            None
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "std")]
    #[test]
    fn test_sam_reader() -> Result<(), EtError> {
        use std::fs::File;

        let f = File::open("tests/data/test.sam")?;
        let rb = ReadBuffer::new(Box::new(&f))?;
        let builder = SamReaderBuilder::default();
        let mut reader = builder.to_reader(rb)?;
        if let Some(Record::Sam { query_name, .. }) = reader.next()? {
            assert_eq!(query_name, "SRR062634.1");
        } else {
            panic!("Sam reader returned non-Mz record");
        };

        let mut n_recs = 1;
        while let Some(Record::Sam { .. }) = reader.next()? {
            n_recs += 1;
        }
        assert_eq!(n_recs, 5);
        Ok(())
    }
}
