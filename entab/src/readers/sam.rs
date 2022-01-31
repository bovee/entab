use alloc::borrow::Cow;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use core::marker::Copy;

use crate::buffer::ReadBuffer;
use crate::parsers::{Endian, FromBuffer, FromSlice, NewLine};
use crate::record::StateMetadata;
use crate::EtError;
use crate::{impl_reader, impl_record};

/// The internal state of the BamReader.
#[derive(Debug, Default)]
pub struct BamState {
    references: Vec<(String, usize)>,
}

impl<'r> StateMetadata<'r> for BamState {}

impl<'r> FromBuffer<'r> for BamState {
    type State = ();

    fn from_buffer(
        &mut self,
        rb: &'r mut ReadBuffer,
        _state: Self::State,
    ) -> Result<bool, EtError> {
        // read the magic & header length, and then the header
        if rb.extract::<&[u8]>(4)? != b"BAM\x01" {
            return Err("Not a valid BAM file".into());
        }
        let header_len = rb.extract::<u32>(Endian::Little)? as usize;
        // TODO: we should read the headers and pass them along
        // to the Reader as metadata once we support that
        let _ = rb.extract::<&[u8]>(header_len);

        // read the reference sequence data
        let mut n_references = rb.extract::<u32>(Endian::Little)? as usize;

        let mut references = Vec::new();
        while n_references > 0 {
            let name_len = rb.extract::<u32>(Endian::Little)? as usize;
            let mut raw_ref_name = rb.extract::<&[u8]>(name_len)?;
            if raw_ref_name.last() == Some(&b'\x00') {
                raw_ref_name = &raw_ref_name[..name_len - 1]
            };
            let ref_name = String::from(alloc::str::from_utf8(raw_ref_name)?);
            let ref_len = rb.extract::<u32>(Endian::Little)? as usize;
            references.push((ref_name, ref_len));
            n_references -= 1;
        }
        self.references = references;
        Ok(true)
    }
}

/// A single record from a BAM file.
#[derive(Debug, Default)]
pub struct BamRecord<'r> {
    /// The name of the mapped sequence.
    pub query_name: &'r str,
    /// Bitvector of flags with information about the mapping.
    pub flag: u16,
    /// The name of the reference mapped to.
    pub ref_name: &'r str,
    /// The position of the mapping, if present.
    pub pos: Option<u64>,
    /// The quality of the mapping, if present.
    pub mapq: Option<u8>,
    /// A abbreviated format indicating how the query maps to the reference.
    ///
    /// `I` - Insertion
    /// `D` - Deletion
    /// `H` - Hard-clipped
    /// `S` - Soft-clipped,
    /// `M` - Match (may be either a `=` or an `X`),
    /// `=` - Identical match
    /// `X` - Near-match (e.g. a SNP)
    pub cigar: Vec<u8>,
    /// Next read's name
    pub rnext: &'r str,
    /// Position of the next read's alignment
    pub pnext: Option<u32>,
    /// Template length
    pub tlen: i32,
    /// The sequence of the query, if present.
    pub seq: Vec<u8>,
    /// The quality scores of the query, if present.
    pub qual: Vec<u8>,
    /// Extra metadata about the mapping.
    pub extra: Cow<'r, [u8]>,
}

impl_record!(BamRecord<'r>: query_name, flag, ref_name, pos, mapq, cigar, rnext, pnext, tlen, seq, qual, extra);

impl<'r> FromBuffer<'r> for BamRecord<'r> {
    type State = &'r mut BamState;

    fn from_buffer(&mut self, rb: &'r mut ReadBuffer, state: Self::State) -> Result<bool, EtError> {
        // each record in a BAM is a different gzip chunk so we
        // have to do a refill before each record
        rb.refill()?;
        if rb.is_empty() && rb.eof {
            return Ok(false);
        }

        // now read the record itself
        let record_len = rb.extract::<u32>(Endian::Little)? as usize;
        if record_len < 32 {
            return Err(EtError::new("Record is unexpectedly short", rb));
        }
        let raw_ref_name_id: i32 = rb.extract(Endian::Little)?;
        self.ref_name = if raw_ref_name_id < 0 {
            ""
        } else if raw_ref_name_id as usize >= state.references.len() {
            return Err(EtError::new("Invalid reference sequence ID", rb));
        } else {
            &state.references[raw_ref_name_id as usize].0
        };
        let raw_pos: i32 = rb.extract(Endian::Little)?;
        self.pos = if raw_pos == -1 {
            None
        } else {
            Some(raw_pos as u64)
        };
        let query_name_len = usize::from(rb.extract::<u8>(Endian::Little)?);
        let raw_mapq: u8 = rb.extract(Endian::Little)?;
        self.mapq = if raw_mapq == 255 {
            None
        } else {
            Some(raw_mapq)
        };
        // don't care about the BAI index bin - &data[10..12]
        let _ = rb.extract::<&[u8]>(2_usize)?;
        let n_cigar_op = usize::from(rb.extract::<u16>(Endian::Little)?);
        self.flag = rb.extract::<u16>(Endian::Little)?;
        let seq_len = rb.extract::<u32>(Endian::Little)? as usize;
        let raw_rnext_id: i32 = rb.extract(Endian::Little)?;
        self.rnext = if raw_rnext_id < 0 {
            ""
        } else if raw_rnext_id as usize >= state.references.len() {
            return Err(EtError::new("Invalid next reference sequence ID", rb));
        } else {
            &state.references[raw_rnext_id as usize].0
        };
        let raw_pnext: i32 = rb.extract(Endian::Little)?;
        self.pnext = if raw_pnext == -1 {
            None
        } else {
            Some(raw_pnext as u32)
        };
        self.tlen = rb.extract::<i32>(Endian::Little)?;

        // now parse the variable length records
        let data = rb.extract::<&[u8]>(record_len - 32)?;

        let mut start = query_name_len;
        if start > data.len() {
            // TODO: use EtError::new ?
            return Err(EtError::from("Invalid query name length"));
        }
        let mut query_name = &data[..start];
        if query_name.last() == Some(&0) {
            query_name = &query_name[..query_name_len - 1]
        }
        self.query_name = alloc::str::from_utf8(query_name)?;

        self.cigar = Vec::new();
        for _ in 0..n_cigar_op {
            let cigar_op = u32::out_of(&data[start..], Endian::Little)? as usize;
            self.cigar.extend((cigar_op >> 4).to_string().as_bytes());
            self.cigar.push(b"MIDNSHP=X"[cigar_op & 7]);
            start += 4;
        }
        if start + seq_len / 2 >= data.len() {
            return Err("Record ended abruptly while reading sequence".into());
        }
        self.seq = vec![0; seq_len];
        for idx in 0..seq_len {
            let byte = data[start + (idx / 2)];
            let byte = usize::from(if idx % 2 == 0 { byte >> 4 } else { byte & 15 });
            self.seq[idx] = b"=ACMGRSVTWYHKDBN"[byte]
        }
        start += (seq_len + 1) / 2;
        self.qual = if data[start] == 255 {
            Vec::new()
        } else {
            let raw_qual = &data[start..start + seq_len];
            raw_qual.iter().map(|m| m + 33).collect()
        };
        // TODO: parse the extra flags some day?
        // self.extra = Cow::Borrowed(b"");
        Ok(true)
    }
}

impl_reader!(BamReader, BamRecord, BamState, ());

/// The internal state of the SamReader.
#[derive(Clone, Copy, Debug, Default)]
pub struct SamState {}

impl<'r> StateMetadata<'r> for SamState {}

impl<'r> FromBuffer<'r> for SamState {
    type State = ();

    fn from_buffer(
        &mut self,
        rb: &'r mut ReadBuffer,
        _state: Self::State,
    ) -> Result<bool, EtError> {
        // eventually we should read the headers and pass them along
        // to the Reader as metadata once we support that
        rb.reserve(1)?;
        while !rb.is_empty() && rb[0] == b'@' {
            if !rb.seek_pattern(b"\n")? {
                break;
            }
            // read the newline too
            let _ = rb.extract::<u8>(Endian::Little)?;
        }

        Ok(true)
    }
}

/// A single record from a SAM file.
#[derive(Debug, Default)]
pub struct SamRecord<'r> {
    /// The name of the mapped sequence.
    pub query_name: &'r str,
    /// Bitvector of flags with information about the mapping.
    pub flag: u16,
    /// The name of the reference mapped to.
    pub ref_name: &'r str,
    /// The position of the mapping, if present.
    pub pos: Option<u64>,
    /// The quality of the mapping, if present.
    pub mapq: Option<u8>,
    /// A abbreviated format indicating how the query maps to the reference.
    ///
    /// `I` - Insertion
    /// `D` - Deletion
    /// `H` - Hard-clipped
    /// `S` - Soft-clipped,
    /// `M` - Match (may be either a `=` or an `X`),
    /// `=` - Identical match
    /// `X` - Near-match (e.g. a SNP)
    pub cigar: &'r [u8],
    /// Next read's name
    pub rnext: &'r str,
    /// Position of the next read's alignment
    pub pnext: Option<u32>,
    /// Template length
    pub tlen: i32,
    /// The sequence of the query, if present.
    pub seq: &'r [u8],
    /// The quality scores of the query, if present.
    pub qual: &'r [u8],
    /// Extra metadata about the mapping.
    pub extra: Cow<'r, [u8]>,
}

impl_record!(SamRecord<'r>: query_name, flag, ref_name, pos, mapq, cigar, rnext, pnext, tlen, seq, qual, extra);

impl<'r> FromBuffer<'r> for SamRecord<'r> {
    type State = &'r mut SamState;

    fn from_buffer(
        &mut self,
        rb: &'r mut ReadBuffer,
        _state: Self::State,
    ) -> Result<bool, EtError> {
        Ok(if let Some(NewLine(line)) = NewLine::get(rb, ())? {
            let chunks: Vec<&[u8]> = line.split(|c| *c == b'\t').collect();
            if chunks.len() < 11 {
                return Err("Sam record too short".into());
            }
            self.query_name = alloc::str::from_utf8(chunks[0])?;
            self.flag = alloc::str::from_utf8(chunks[1])?.parse()?;
            self.ref_name = if chunks[2] == b"*" {
                ""
            } else {
                alloc::str::from_utf8(chunks[2])?
            };
            self.pos = if chunks[3] == b"0" {
                None
            } else {
                // convert to 0-based indexing while we're at it
                let mut val = alloc::str::from_utf8(chunks[3])?.parse()?;
                val -= 1;
                Some(val)
            };
            self.mapq = if chunks[4] == b"255" {
                None
            } else {
                Some(alloc::str::from_utf8(chunks[4])?.parse()?)
            };
            self.cigar = if chunks[5] == b"*" { b"" } else { chunks[5] };
            self.rnext = if chunks[6] == b"*" {
                ""
            } else {
                alloc::str::from_utf8(chunks[6])?
            };
            self.pnext = if chunks[7] == b"0" {
                None
            } else {
                // convert to 0-based indexing while we're at it
                let mut val = alloc::str::from_utf8(chunks[7])?.parse()?;
                val -= 1;
                Some(val)
            };
            self.tlen = alloc::str::from_utf8(chunks[8])?.parse()?;
            self.seq = if chunks[9] == b"*" { b"" } else { chunks[9] };
            self.qual = if chunks[10] == b"*" { b"" } else { chunks[10] };
            self.extra = if chunks.len() == 11 {
                Cow::Borrowed(b"")
            } else if chunks.len() == 12 {
                chunks[11].into()
            } else {
                let mut joined = chunks[11].to_vec();
                for c in &chunks[12..] {
                    joined.push(b'|');
                    joined.extend(*c);
                }
                joined.into()
            };
            true
        } else {
            false
        })
    }
}

impl_reader!(SamReader, SamRecord, SamState, ());

#[cfg(test)]
mod tests {
    use super::*;
    use crate::readers::RecordReader;
    use core::include_bytes;
    static KNOWN_SEQ: &[u8] = b"GGGTTTTCCTGAAAAAGGGATTCAAGAAAGAAAACTTACATGAGGTGATTGTTTAATGTTGCTACCAAAGAAGAGAGAGTTACCTGCCCATTCACTCAGG";

    #[test]
    fn test_sam_reader() -> Result<(), EtError> {
        let rb = ReadBuffer::from_slice(include_bytes!("../../tests/data/test.sam"));
        let mut reader = SamReader::new(rb, ())?;
        let _ = reader.metadata();
        if let Some(SamRecord {
            query_name, seq, ..
        }) = reader.next()?
        {
            assert_eq!(query_name, "SRR062634.1");
            assert_eq!(seq, KNOWN_SEQ);
        } else {
            panic!("Sam reader returned non-Mz record");
        };

        let mut n_recs = 1;
        while let Some(_) = reader.next()? {
            n_recs += 1;
        }
        assert_eq!(n_recs, 5);
        Ok(())
    }

    #[test]
    fn test_sam_no_data() -> Result<(), EtError> {
        let rb = ReadBuffer::from_slice(b"@HD\ttest\n");
        let mut reader = SamReader::new(rb, ())?;
        assert!(reader.next()?.is_none());
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
        let mut reader = BamReader::new(rb, ())?;
        let _ = reader.metadata();

        if let Some(BamRecord {
            query_name, seq, ..
        }) = reader.next()?
        {
            assert_eq!(query_name, "SRR062634.1");
            let known_seq = KNOWN_SEQ.to_vec();
            assert_eq!(seq, known_seq);
        } else {
            panic!("Sam reader returned non-Mz record");
        };

        let mut n_recs = 1;
        while let Some(_) = reader.next()? {
            n_recs += 1;
        }
        assert_eq!(n_recs, 5);
        Ok(())
    }

    #[cfg(all(feature = "compression", feature = "std"))]
    #[test]
    fn test_bam_fuzz_errors() -> Result<(), EtError> {
        let data = [
            66, 65, 77, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 10, 10, 125, 10, 10, 10, 10, 255, 255, 255, 255,
            10, 10, 18,
        ];
        let rb = ReadBuffer::from_slice(&data);
        let mut reader = BamReader::new(rb, ())?;
        assert!(reader.next().is_err());

        let data = [
            66, 65, 77, 1, 62, 1, 0, 0, 0, 0, 0, 0, 12, 10, 255, 255, 255, 255, 255, 116, 116, 116,
            246, 245, 245, 240, 10, 62, 8, 10, 255, 255, 255, 251, 255, 255, 255, 255, 255, 181,
            181, 181, 181, 181, 181, 181, 117, 117, 117, 117, 117, 117, 181, 117, 117, 10, 10, 10,
            10, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117,
            117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 181, 117, 117,
            10, 10, 10, 10, 10, 10, 10, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117,
            117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117,
            117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117,
            117, 117, 181, 117, 117, 10, 10, 10, 10, 10, 10, 10, 10, 10, 62, 10, 10, 0, 1, 0, 0, 0,
            0, 0, 0, 0, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117,
            117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 10, 10, 10, 62, 10, 10, 117, 117,
            117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 181, 117, 117,
            10, 10, 10, 10, 10, 10, 10, 10, 10, 62, 10, 10, 0, 1, 0, 0, 0, 0, 0, 0, 0, 117, 117,
            117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117,
            117, 117, 117, 117, 117, 117, 10, 10, 10, 62, 10, 10, 117, 117, 117, 117, 117, 117,
            117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 10, 10, 10, 62, 10, 10, 117,
            117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 181, 117,
            117, 10, 10, 10, 10, 10, 10, 10, 10, 10, 62, 10, 10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 117,
            117, 117, 117, 117, 117, 117, 117, 117, 62, 10, 10,
        ];
        let rb = ReadBuffer::from_slice(&data);
        assert!(BamReader::new(rb, ()).is_err());

        let data = [
            66, 65, 77, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 105, 0, 110, 0, 0, 0, 0,
        ];
        let rb = ReadBuffer::from_slice(&data);
        let mut reader = BamReader::new(rb, ())?;
        assert!(reader.next().is_err());

        let data = [
            66, 65, 77, 1, 62, 1, 0, 0, 0, 0, 254, 254, 254, 254, 254, 254, 254, 254, 254, 254,
            254, 254, 254, 254, 254, 254, 254, 254, 254, 254, 254, 252, 254, 254, 254, 254, 254,
            254, 254, 254, 254, 254, 254, 254, 138, 138, 138, 138, 138, 227, 10, 10, 14, 10, 20,
            10, 10, 10, 10, 62, 10, 249, 62, 10, 200, 62, 10, 134, 62, 10, 10, 10, 255, 255, 255,
            255, 138, 138, 138, 138, 138, 138, 116, 117, 138, 138, 138, 1, 0, 138, 138, 138, 138,
            138, 138, 138, 138, 138, 139, 139, 116, 116, 116, 116, 116, 246, 245, 245, 240, 138,
            138, 138, 138, 0, 0, 0, 0, 0, 255, 0, 35, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 62, 10, 227, 205, 205, 205, 110, 239, 10, 42, 10, 10, 116, 116, 116, 116, 116,
            116, 169, 77, 86, 139, 139, 116, 116, 116, 116, 116, 246, 245, 245, 240, 10, 10, 116,
            116, 116, 174, 90, 10, 10, 116, 116, 116, 116, 116, 116, 169, 77, 86, 139, 139, 116,
            116, 116, 116, 116, 246, 245, 245, 240, 116, 116, 116, 174, 90, 84, 82, 13, 10, 26, 10,
            116, 116, 116, 116, 116, 246, 245, 245, 240, 0, 0, 0, 0, 255, 0, 35, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 62, 10, 227, 205, 205, 205, 110, 239, 10, 42, 10, 10,
            116, 116, 116, 116, 116, 116, 169, 77, 86, 139, 139, 116, 116, 116, 116, 116, 246, 245,
            245, 240, 10, 10, 116, 116, 116, 116, 116, 116, 169, 77, 86, 139, 139, 116, 116, 116,
            116, 116, 246, 245, 245, 240, 116, 116, 116, 116, 116, 246, 245, 245, 240, 0, 0, 0, 0,
            255, 0, 35, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 62, 10, 227, 205, 205,
            205, 110, 239, 10, 42, 10, 10, 116, 116, 116, 116, 116, 116, 116, 169, 77, 86, 139,
            139, 116, 116, 116, 116, 116, 246, 245, 245, 240,
        ];
        let rb = ReadBuffer::from_slice(&data);
        let mut reader = BamReader::new(rb, ())?;
        assert!(reader.next().is_err());

        let data = [
            66, 65, 77, 1, 62, 1, 0, 0, 0, 0, 0, 0, 12, 10, 255, 255, 255, 255, 223, 223, 223, 223,
            223, 223, 223, 223, 223, 223, 223, 223, 223, 223, 223, 223, 223, 223, 223, 223, 223,
            223, 223, 223, 223, 223, 223, 223, 223, 223, 223, 223, 223, 0, 0, 0, 10, 10, 10, 10,
            10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 62, 10, 62, 10, 134, 10, 62, 10, 10, 10,
            10, 0, 0, 0, 0, 0, 0, 0, 4, 10, 10, 103, 10, 10, 10, 181, 181, 181, 181, 181, 181, 181,
            181, 62, 10, 10, 10, 10, 10, 10, 10, 68, 61, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10,
            10, 10, 10, 10, 10, 10, 10, 10, 107, 181, 181, 181, 181, 181, 181, 181, 181, 181, 181,
            181, 181, 181, 181, 181, 181, 181, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 5, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 35, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 223, 223, 223, 223, 223, 223, 223, 223, 223, 223, 223, 223,
            223, 223, 223, 223, 223, 223, 223, 223, 223, 223, 223, 223, 223, 223, 223, 223, 223,
            223, 223, 223, 223, 223, 223, 223, 223, 223, 223, 223, 0, 0, 0, 10, 10, 10, 10, 10, 10,
            10, 10, 10, 10, 10, 10, 10, 10, 10, 62, 10, 181, 181, 181, 181, 181, 181, 181, 181,
            181, 181, 181, 181, 181, 181, 181, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 5, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 35, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 223, 223, 223, 223, 223, 223, 223, 223, 223, 223, 223, 223, 223,
            223, 223, 223, 223, 223, 223, 223, 223, 223, 223, 223, 223, 223, 223, 223, 223, 223,
            223, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 62, 10,
        ];
        let rb = ReadBuffer::from_slice(&data);
        let mut reader = BamReader::new(rb, ())?;
        assert!(reader.next().is_err());

        let data = [
            66, 65, 77, 1, 62, 1, 0, 0, 0, 0, 0, 0, 12, 0, 0, 0, 0, 0, 0, 1, 209, 255, 255, 122,
            255, 255, 255, 255, 138, 138, 138, 138, 138, 138, 0, 0, 138, 138, 138, 138, 138, 138,
            138, 138, 138, 138, 138, 223, 223, 223, 223, 223, 223, 223, 223, 223, 223, 223, 223,
            223, 0, 0, 0, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 62, 10, 10,
            10, 10, 10, 10, 62, 0, 0, 10, 10, 10, 103, 10, 10, 10, 181, 181, 181, 181, 181, 181,
            181, 181, 62, 10, 10, 10, 10, 10, 10, 10, 68, 61, 10, 10, 10, 10, 10, 10, 10, 10, 10,
            10, 10, 10, 10, 10, 10, 10, 10, 10, 107, 181, 181, 181, 181, 181, 181, 181, 181, 181,
            181, 181, 181, 181, 181, 181, 181, 181, 0, 0, 0, 0, 0, 16, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 5, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 35, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 223, 223, 223, 223, 223, 223, 223, 223, 223, 223, 223, 223, 223, 223, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 223, 223, 223, 223, 223, 223, 223, 223, 223, 223, 223,
            223, 223, 223, 223, 223, 223, 223, 223, 223, 223, 223, 223, 0, 0, 10, 10, 10, 10, 10,
            10, 10, 10, 10, 10, 10, 10, 107, 181, 181, 181, 181, 181, 181, 181, 181, 181, 181, 181,
            181, 181, 181, 181, 181, 181, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            5, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 35, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 223, 223, 223, 223, 223, 223, 223, 223, 223, 223, 223, 223, 223, 223, 223, 223, 223,
            185, 255, 255, 255, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10,
        ];
        let rb = ReadBuffer::from_slice(&data);
        let mut reader = BamReader::new(rb, ())?;
        assert!(reader.next().is_err());

        let data = [
            66, 65, 77, 1, 62, 1, 0, 0, 5, 157, 0, 0, 12, 10, 255, 255, 255, 255, 255, 255, 255,
            255, 255, 138, 138, 138, 138, 138, 138, 138, 138, 138, 138, 138, 138, 138, 138, 138,
            138, 138, 138, 138, 138, 138, 126, 117, 117, 117, 138, 138, 138, 138, 138, 255, 255,
            255, 255, 255, 255, 255, 255, 255, 10, 10, 20, 10, 10, 10, 10, 62, 10, 200, 62, 10,
            134, 10, 62, 10, 10, 10, 10, 10, 157, 10, 10, 62, 0, 0, 10, 10, 10, 103, 10, 10, 117,
            117, 117, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
            255, 255, 253, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 0, 0, 246, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 64, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 70, 0, 0, 0, 0, 156, 156, 156, 156, 156, 156, 156, 156, 156, 156,
            156, 156, 156, 156, 156, 156, 156, 156, 156, 156, 156, 156, 156, 156, 156, 156, 156,
            156, 156, 156, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255,
            255, 255, 255, 255, 255, 0, 0, 246, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 64, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 70, 0, 0, 0, 0, 156,
            156, 156, 156, 156, 156, 156, 156, 156, 156, 156, 156, 156, 156, 156, 156, 156, 156,
            156, 156, 156, 156, 156, 156, 156, 156, 156, 156, 156, 156, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 255,
        ];
        let rb = ReadBuffer::from_slice(&data);
        let mut reader = BamReader::new(rb, ())?;
        assert!(reader.next().is_err());

        let data = [
            66, 65, 77, 1, 255, 255, 255, 1, 0, 0, 0, 0, 62, 1, 0, 0, 0, 0, 254, 254, 254, 254,
            168, 168, 255, 168, 255, 1, 0, 0, 0, 0, 62, 1, 0, 0, 0, 0, 254, 254, 254, 254, 168,
            168, 255, 168, 168, 168, 168, 168, 168, 168, 168, 168, 168, 168, 168, 168, 61, 168,
            168, 168, 168, 168, 168, 168, 168, 168, 168, 168, 168, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 155,
            155, 155, 155, 155, 155, 155, 155, 155, 155, 10, 10, 10, 10, 10, 10, 10, 1, 161, 70, 0,
            105, 0, 110, 0, 57, 10, 75, 75, 75, 75, 75, 75, 75, 75, 75, 81, 101, 41, 192, 45, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 191, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 62, 10, 10,
            10, 155, 155, 155, 155, 155, 159, 155, 155, 155, 155, 155, 155, 155, 155, 155, 155,
            155, 155, 155, 155, 155, 155, 155, 155, 155, 155, 155, 10, 10, 10, 10, 10, 10, 10, 1,
            161, 70, 0, 105, 0, 110, 0, 57, 10, 10, 75, 75, 75, 75, 75, 75, 75, 75, 75, 81, 101,
            41, 192, 45, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 62, 10, 10, 10,
            155, 155, 155, 155, 155, 159, 155, 155, 155, 155, 155, 155, 155, 66, 62, 1, 0, 155,
            155, 155, 155, 155, 155, 155, 155, 155, 155, 10, 10, 10, 10, 10, 10, 10, 1, 161, 70, 0,
            105, 0, 110, 0, 57, 10,
        ];
        let rb = ReadBuffer::from_slice(&data);
        let mut reader = BamReader::new(rb, ())?;
        assert!(reader.next().is_err());

        Ok(())
    }
}
