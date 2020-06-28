use alloc::borrow::Cow;

use byteorder::{ByteOrder, LittleEndian};

use crate::buffer::ReadBuffer;
use crate::record::{ReaderBuilder, Record, RecordReader};
use crate::EtError;

pub struct BamReaderBuilder;

impl Default for BamReaderBuilder {
    fn default() -> Self {
        BamReaderBuilder
    }
}

impl ReaderBuilder for BamReaderBuilder {
    fn to_reader<'r>(&self, mut rb: ReadBuffer<'r>) -> Result<Box<dyn RecordReader + 'r>, EtError> {
        // read the magic & header length, and then the header
        rb.reserve(8)?;
        let data = rb.partial_consume(8);
        if &data[..4] != b"BAM\x01" {
            return Err("Not a valid BAM file".into());
        }
        let header_len = LittleEndian::read_u32(&data[4..8]) as usize;
        rb.reserve(header_len + 8)?;
        let _ = rb.partial_consume(header_len);

        // read the reference sequence data
        let data = rb.partial_consume(4);
        let mut n_references = LittleEndian::read_u32(&data) as usize;
        while n_references > 0 {
            rb.reserve(4)?;
            let name_len = LittleEndian::read_u32(rb.partial_consume(4)) as usize;
            rb.reserve(name_len + 4)?;
            let _ = rb.partial_consume(name_len);
            let _ = rb.partial_consume(4);
            n_references -= 1;
        }

        // TODO: we should read the headers and pass them along
        // to the Reader as metadata once we support that and also
        // we need them to print the `ref_name` and `rnext` fields
        Ok(Box::new(BamReader { rb }))
    }
}

fn bytes_to_bam(data: &[u8]) -> Result<Record, EtError> {
    let raw_ref_name_id = LittleEndian::read_i32(&data[0..4]);
    let ref_name = if raw_ref_name_id < 0 {
        ""
    } else {
        // TODO: raw_ref_name_id to ref_name
        ""
    };
    let raw_pos = LittleEndian::read_i32(&data[4..8]);
    let pos = if raw_pos == -1 {
        None
    } else {
        Some(raw_pos as u64)
    };
    let query_name_len = usize::from(data[8]);
    let mapq = if data[9] == 255 { None } else { Some(data[9]) };
    // don't care about the BAI index bin - &data[10..12]
    let n_cigar_op = usize::from(LittleEndian::read_u16(&data[12..14]));
    let flag = LittleEndian::read_u16(&data[14..16]);
    let seq_len = LittleEndian::read_u32(&data[16..20]) as usize;
    let raw_rnext_id = LittleEndian::read_i32(&data[20..24]);
    let rnext = if raw_rnext_id < 0 {
        ""
    } else {
        // TODO: raw_rnext_id to next reference name
        ""
    };
    let raw_pnext = LittleEndian::read_i32(&data[24..28]);
    let pnext = if raw_pnext == -1 {
        None
    } else {
        Some(raw_pnext as u32)
    };
    let tlen = LittleEndian::read_i32(&data[28..32]);

    // now parse the variable length records
    let mut start = 32 + query_name_len;
    let mut query_name = &data[32..start];
    if query_name.last() == Some(&0) {
        query_name = &query_name[..query_name_len - 1]
    }
    let mut cigar: Vec<u8> = Vec::new();
    for _ in 0..n_cigar_op {
        let cigar_op = LittleEndian::read_u32(&data[start..start + 4]) as usize;
        cigar.extend((cigar_op >> 4).to_string().as_bytes());
        cigar.push(b"MIDNSHP=X"[cigar_op & 7]);
        start += 4;
    }
    let mut seq = vec![0; seq_len];
    for idx in 0..seq_len {
        let byte = data[start + (idx / 2)];
        let byte = usize::from(if idx % 2 == 0 { byte >> 4 } else { byte & 15 });
        seq[idx] = b"=ACMGRSVTWYHKDBN"[byte]
    }
    start += (seq_len + 1) / 2;
    let qual = &data[start..start + seq_len];

    Ok(Record::Sam {
        query_name: alloc::str::from_utf8(query_name)?,
        flag,
        ref_name,
        pos,
        mapq,
        cigar: Cow::Owned(cigar),
        rnext,
        pnext,
        tlen,
        seq: Cow::Owned(seq),
        qual,
        // TODO: parse the extra flags some day?
        extra: Cow::Borrowed(b""),
    })
}

pub struct BamReader<'r> {
    rb: ReadBuffer<'r>,
}

impl<'r> RecordReader for BamReader<'r> {
    fn headers(&self) -> Vec<&str> {
        // FIXME: need header names
        vec![]
    }

    fn next(&mut self) -> Result<Option<Record>, EtError> {
        // each record in a BAM is a different gzip chunk so we
        // have to do a refill before each record
        self.rb.refill()?;
        if self.rb.is_empty() && self.rb.eof {
            return Ok(None);
        }

        // now read the record itself
        let buffer_pos = (self.rb.reader_pos, self.rb.record_pos);
        self.rb.reserve(4)?;
        let rec_len = LittleEndian::read_u32(self.rb.partial_consume(4)) as usize;
        self.rb.reserve(rec_len)?;
        let record = bytes_to_bam(self.rb.consume(rec_len)).map_err(|mut e| {
            // we can't use `fill_pos` b/c that touchs the buffer
            // and messes up the lifetimes :/
            e.byte = Some(buffer_pos.0);
            e.record = Some(buffer_pos.1 + 1);
            e
        })?;
        Ok(Some(record))
    }
}

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
        ""
    } else {
        alloc::str::from_utf8(chunks[6])?
    };
    let pnext = if chunks[7] == b"0" {
        None
    } else {
        // convert to 0-based indexing while we're at it
        let mut val = alloc::str::from_utf8(chunks[7])?.parse()?;
        val -= 1;
        Some(val)
    };
    let seq = if chunks[9] == b"*" {
        Cow::Borrowed(&b""[..])
    } else {
        chunks[9].into()
    };
    let qual = if chunks[10] == b"*" { b"" } else { chunks[10] };
    Ok(Record::Sam {
        query_name: alloc::str::from_utf8(chunks[0])?,
        flag: alloc::str::from_utf8(chunks[1])?.parse()?,
        ref_name: alloc::str::from_utf8(chunks[2])?,
        pos,
        mapq,
        cigar: chunks[5].into(),
        rnext,
        pnext,
        tlen: alloc::str::from_utf8(chunks[8])?.parse()?,
        seq,
        qual,
        extra: chunks[11].into(),
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
    static KNOWN_SEQ: &[u8] = b"GGGTTTTCCTGAAAAAGGGATTCAAGAAAGAAAACTTACATGAGGTGATTGTTTAATGTTGCTACCAAAGAAGAGAGAGTTACCTGCCCATTCACTCAGG";

    #[cfg(feature = "std")]
    #[test]
    fn test_sam_reader() -> Result<(), EtError> {
        use std::fs::File;

        let f = File::open("tests/data/test.sam")?;
        let rb = ReadBuffer::new(Box::new(&f))?;
        let builder = SamReaderBuilder::default();
        let mut reader = builder.to_reader(rb)?;
        if let Some(Record::Sam {
            query_name, seq, ..
        }) = reader.next()?
        {
            assert_eq!(query_name, "SRR062634.1");
            assert_eq!(seq, Cow::Borrowed(KNOWN_SEQ));
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

    #[cfg(all(feature = "compression", feature = "std"))]
    #[test]
    fn test_bam_reader() -> Result<(), EtError> {
        use std::fs::File;

        use crate::compression::decompress;
        use crate::filetype::FileType;

        let f = File::open("tests/data/test.bam")?;
        let (stream, filetype, compress) = decompress(Box::new(&f))?;
        assert_eq!(filetype, FileType::Bam);
        assert_eq!(compress, Some(FileType::Gzip));
        let rb = ReadBuffer::new(stream)?;
        let builder = BamReaderBuilder::default();
        let mut reader = builder.to_reader(rb)?;

        if let Some(Record::Sam {
            query_name, seq, ..
        }) = reader.next()?
        {
            assert_eq!(query_name, "SRR062634.1");
            let known_seq: Cow<[u8]> = Cow::Owned(KNOWN_SEQ.to_vec());
            assert_eq!(seq, known_seq);
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