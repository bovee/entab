use alloc::borrow::Cow;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use core::convert::TryFrom;
use core::marker::Copy;

use crate::parsers::{extract, extract_opt, unsafe_access_state, Endian, FromSlice, NewLine, Skip};
use crate::record::StateMetadata;
use crate::EtError;
use crate::{impl_reader, impl_record};

/// The internal state of the `BamReader`.
#[derive(Debug, Default)]
pub struct BamState {
    references: Vec<(String, usize)>,
}

impl StateMetadata for BamState {
    fn header(&self) -> Vec<&str> {
        vec![
            "query_name",
            "ref_name",
            "pos",
            "mapq",
            "cigar",
            "rnext",
            "pnext",
            "tlen",
            "seq",
            "qual",
            "extra",
        ]
    }
}

impl<'r> FromSlice<'r> for BamState {
    type State = ();

    fn parse(
        rb: &[u8],
        eof: bool,
        consumed: &mut usize,
        _state: &mut Self::State,
    ) -> Result<bool, EtError> {
        let con = &mut 0;
        // read the magic & header length, and then the header
        if extract::<&[u8]>(rb, con, 4)? != b"BAM\x01" {
            return Err("Not a valid BAM file".into());
        }
        let mut header_len = extract::<u32>(rb, con, Endian::Little)? as usize;
        let _ = Skip::parse(rb, eof, con, &mut header_len)?;

        // read the reference sequence data
        let mut n_references = extract::<u32>(rb, con, Endian::Little)? as usize;
        while n_references > 0 {
            let name_len = extract::<u32>(rb, con, Endian::Little)? as usize;
            let _ = Skip::parse(rb, eof, con, &mut (4 + name_len))?;
            n_references -= 1;
        }
        *consumed += *con;
        Ok(true)
    }

    fn get(&mut self, rb: &'r [u8], _state: &Self::State) -> Result<(), EtError> {
        let con = &mut 0;
        let _ = extract::<Skip>(rb, con, 4)?;
        let header_len = extract::<u32>(rb, con, Endian::Little)? as usize;
        // TODO: we should read the headers and pass them along
        // to the Reader as metadata once we support that
        drop(extract::<Skip>(rb, con, header_len));

        // read the reference sequence data
        let mut n_references = extract::<u32>(rb, con, Endian::Little)? as usize;

        let mut references = Vec::new();
        while n_references > 0 {
            let name_len = extract::<u32>(rb, con, Endian::Little)? as usize;
            let mut raw_ref_name = extract::<&[u8]>(rb, con, name_len)?;
            if raw_ref_name.last() == Some(&b'\x00') {
                raw_ref_name = &raw_ref_name[..name_len - 1];
            };
            let ref_name = String::from(alloc::str::from_utf8(raw_ref_name)?);
            let ref_len = extract::<u32>(rb, con, Endian::Little)? as usize;
            references.push((ref_name, ref_len));
            n_references -= 1;
        }
        self.references = references;
        Ok(())
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

impl<'r> FromSlice<'r> for BamRecord<'r> {
    type State = &'r mut BamState;

    fn parse(
        rb: &[u8],
        eof: bool,
        consumed: &mut usize,
        _state: &mut Self::State,
    ) -> Result<bool, EtError> {
        // each record in a BAM is a different gzip chunk so we
        // have to do a refill before each record
        if rb.is_empty() {
            if eof {
                return Ok(false);
            }
            return Err(EtError::new("BAM file is incomplete").incomplete());
        }
        // now read the record itself
        let con = &mut 0;
        let mut record_len = extract::<u32>(rb, con, Endian::Little)? as usize;
        if record_len < 32 {
            return Err("Record is unexpectedly short".into());
        }
        let _ = Skip::parse(rb, eof, con, &mut record_len)?;
        *consumed += *con;

        Ok(true)
    }

    fn get(&mut self, rb: &'r [u8], state: &Self::State) -> Result<(), EtError> {
        let con = &mut 0;
        let record_len = extract::<u32>(rb, con, Endian::Little)? as usize;

        let raw_ref_name_id: i32 = extract(rb, con, Endian::Little)?;
        self.ref_name = if raw_ref_name_id < 0 {
            ""
        } else if usize::try_from(raw_ref_name_id)? >= state.references.len() {
            return Err("Invalid reference sequence ID".into());
        } else {
            &unsafe_access_state(state).references[usize::try_from(raw_ref_name_id)?].0
        };
        let raw_pos: i32 = extract(rb, con, Endian::Little)?;
        self.pos = if raw_pos == -1 {
            None
        } else {
            Some(u64::try_from(raw_pos)?)
        };
        let query_name_len = usize::from(extract::<u8>(rb, con, Endian::Little)?);
        let raw_mapq: u8 = extract(rb, con, Endian::Little)?;
        self.mapq = if raw_mapq == 255 {
            None
        } else {
            Some(raw_mapq)
        };
        // don't care about the BAI index bin - &data[10..12]
        let _ = extract::<&[u8]>(rb, con, 2_usize)?;
        let n_cigar_op = usize::from(extract::<u16>(rb, con, Endian::Little)?);
        self.flag = extract::<u16>(rb, con, Endian::Little)?;
        let seq_len = extract::<u32>(rb, con, Endian::Little)? as usize;
        let raw_rnext_id: i32 = extract(rb, con, Endian::Little)?;
        self.rnext = if raw_rnext_id < 0 {
            ""
        } else if usize::try_from(raw_rnext_id)? >= state.references.len() {
            return Err("Invalid next reference sequence ID".into());
        } else {
            &unsafe_access_state(state).references[usize::try_from(raw_rnext_id)?].0
        };
        let raw_pnext: i32 = extract(rb, con, Endian::Little)?;
        self.pnext = if raw_pnext == -1 {
            None
        } else {
            Some(u32::try_from(raw_pnext)?)
        };
        self.tlen = extract::<i32>(rb, con, Endian::Little)?;

        // now parse the variable length records
        let data = extract::<&[u8]>(rb, con, record_len - 32)?;
        if query_name_len + n_cigar_op * 8 + (1.5 * seq_len as f32 + 1.).ceil() as usize
            > data.len()
        {
            // there's not enough space for the query name, cigar, and sequence/quality?
            return Err("Record ended abruptly while reading variable-length data".into());
        }

        let mut start = query_name_len;
        let mut query_name = &data[..start];
        if query_name.last() == Some(&0) {
            query_name = &query_name[..query_name_len - 1];
        }
        self.query_name = alloc::str::from_utf8(query_name)?;

        self.cigar = Vec::new();
        for _ in 0..n_cigar_op {
            let cigar_op = extract::<u32>(data, &mut start, Endian::Little)? as usize;
            self.cigar.extend((cigar_op >> 4).to_string().as_bytes());
            self.cigar.push(b"MIDNSHP=X"[cigar_op & 7]);
            start += 4;
        }
        self.seq = vec![0; seq_len];
        for idx in 0..seq_len {
            let byte = data[start + (idx / 2)];
            let byte = usize::from(if idx % 2 == 0 { byte >> 4 } else { byte & 15 });
            self.seq[idx] = b"=ACMGRSVTWYHKDBN"[byte];
        }
        start += (seq_len + 1) / 2;
        self.qual = if data[start] == 255 {
            Vec::new()
        } else {
            let raw_qual = &data[start..start + seq_len];
            raw_qual.iter().map(|m| m.saturating_add(33)).collect()
        };
        // TODO: parse the extra flags some day?
        // self.extra = Cow::Borrowed(b"");
        Ok(())
    }
}

impl_reader!(BamReader, BamRecord, BamState, ());

/// The internal state of the `SamReader`.
#[derive(Clone, Copy, Debug, Default)]
pub struct SamState {}

impl StateMetadata for SamState {
    fn header(&self) -> Vec<&str> {
        vec![
            "query_name",
            "flag",
            "ref_name",
            "pos",
            "mapq",
            "cigar",
            "rnext",
            "pnext",
            "tlen",
            "seq",
            "qual",
            "extra",
        ]
    }
}

impl<'r> FromSlice<'r> for SamState {
    type State = ();

    fn parse(
        rb: &[u8],
        eof: bool,
        consumed: &mut usize,
        _state: &mut Self::State,
    ) -> Result<bool, EtError> {
        // eventually we should read the headers and pass them along
        // to the Reader as metadata once we support that
        let con = &mut 0;
        // we're using `to_read` to keep track of how much *only* the header lines take up since
        // the final extracted line we don't want to consumed
        let mut to_read = 0;
        while let Some(header) = extract_opt::<NewLine>(rb, eof, con, 0)? {
            if header.0.get(0) != Some(&b'@') {
                break;
            }
            to_read = *con;
        }
        *consumed += to_read;

        Ok(true)
    }

    fn get(&mut self, _buf: &'r [u8], _state: &Self::State) -> Result<(), EtError> {
        Ok(())
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

impl<'r> FromSlice<'r> for SamRecord<'r> {
    type State = &'r mut SamState;

    fn parse(
        rb: &[u8],
        eof: bool,
        consumed: &mut usize,
        _state: &mut Self::State,
    ) -> Result<bool, EtError> {
        let con = &mut 0;
        Ok(match extract_opt::<NewLine>(rb, eof, con, 0)? {
            Some(_) => {
                *consumed += *con;
                true
            }
            None => false,
        })
    }

    fn get(&mut self, buf: &'r [u8], _state: &Self::State) -> Result<(), EtError> {
        // TODO: need to remove terminal newline?
        let chunks: Vec<&[u8]> = buf.split(|c| *c == b'\t').collect();
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
        let pos: u64 = alloc::str::from_utf8(chunks[3])?.parse()?;
        self.pos = if pos == 0 {
            None
        } else {
            // convert to 0-based indexing while we're at it
            Some(pos - 1)
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
        let pnext: u32 = alloc::str::from_utf8(chunks[7])?.parse()?;
        self.pnext = if pnext == 0 {
            None
        } else {
            // convert to 0-based indexing while we're at it
            Some(pnext - 1)
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
        Ok(())
    }
}

impl_reader!(SamReader, SamRecord, SamState, ());

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(all(feature = "compression", feature = "std"))]
    use crate::buffer::ReadBuffer;
    use crate::readers::RecordReader;

    use core::include_bytes;
    static KNOWN_SEQ: &[u8] = b"GGGTTTTCCTGAAAAAGGGATTCAAGAAAGAAAACTTACATGAGGTGATTGTTTAATGTTGCTACCAAAGAAGAGAGAGTTACCTGCCCATTCACTCAGG";

    #[test]
    fn test_sam_reader() -> Result<(), EtError> {
        let rb = include_bytes!("../../tests/data/test.sam");
        let mut reader = SamReader::new(&rb[..], ())?;
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
        while reader.next()?.is_some() {
            n_recs += 1;
        }
        assert_eq!(n_recs, 5);
        Ok(())
    }

    #[test]
    fn test_sam_no_data() -> Result<(), EtError> {
        let data = b"@HD\ttest\n";
        let mut reader = SamReader::new(&data[..], ())?;
        assert!(reader.next()?.is_none());
        Ok(())
    }

    #[test]
    fn test_sam_bad_fuzzes() -> Result<(), EtError> {
        const TEST_SAM: &[u8] = b"@HD\t\n\n";
        let mut reader = SamReader::new(TEST_SAM, ())?;
        assert!(reader.next().is_err());

        // test with a pos of "00" instead of "0"
        let data = b"@HD\t\x1a{\n\x1a\t0\t\t00\t\t\t\t\t\t\t";
        let mut reader = SamReader::new(&data[..], ())?;
        assert!(reader.next().is_err());

        // this one was just slow?
        let data = b"@HD\t\n\x1a\t10\t*\t0\t0\ty\t*\t1\t200\t\t0\0\n\x1a\t00\t*\t0\t0\t\t\t0\t201\t\t0\t\0\n\x1a\t0\t*\t0\t0\tyy;\t*\t0\t200\t\t0\0\n\x1a\t00`\t*\t0\t0\t$\t*\t200I\t\t0\tyy";
        let mut reader = SamReader::new(&data[..], ())?;
        assert!(reader.next()?.is_some());
        assert!(reader.next()?.is_some());
        assert!(reader.next()?.is_some());
        assert!(reader.next().is_err());

        Ok(())
    }

    #[cfg(all(feature = "compression", feature = "std"))]
    #[test]
    fn test_bam_reader() -> Result<(), EtError> {
        use std::fs::File;

        use crate::compression::decompress;
        use crate::filetype::FileType;

        let f = File::open("tests/data/test.bam")?;
        let (stream, filetype, compress) = decompress(Box::new(f))?;
        assert_eq!(filetype, FileType::Bam);
        assert_eq!(compress, Some(FileType::Gzip));
        let rb = ReadBuffer::from_reader(stream, None)?;
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
        while reader.next()?.is_some() {
            n_recs += 1;
        }
        assert_eq!(n_recs, 5);
        Ok(())
    }

    #[cfg(all(feature = "compression", feature = "std"))]
    #[test]
    fn test_bam_fuzz_errors() -> Result<(), EtError> {
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
        assert!(BamReader::new(&data[..], ()).is_err());

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
        let mut reader = BamReader::new(&data[..], ())?;
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
        let mut reader = BamReader::new(&data[..], ())?;
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
        assert!(BamReader::new(&data[..], ()).is_err());

        let data = [
            66, 65, 77, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 10, 10, 125, 10, 10, 10, 10, 255, 255, 255, 255,
            10, 10, 18,
        ];
        assert!(BamReader::new(&data[..], ()).is_err());

        let data = [
            66, 65, 77, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 105, 0, 110, 0, 0, 0, 0,
        ];
        assert!(BamReader::new(&data[..], ()).is_err());

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
        assert!(BamReader::new(&data[..], ()).is_err());

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
        assert!(BamReader::new(&data[..], ()).is_err());

        let data = [
            66, 65, 77, 1, 62, 1, 0, 0, 0, 0, 0, 0, 12, 10, 255, 255, 255, 255, 255, 255, 255, 255,
            255, 122, 255, 255, 255, 138, 255, 138, 138, 138, 138, 138, 138, 138, 138, 138, 138,
            138, 138, 138, 138, 138, 138, 138, 138, 138, 138, 138, 138, 138, 138, 138, 138, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 32, 0, 0, 0, 0, 0,
            74, 10, 10, 10, 10, 10, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117,
            117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 255, 255, 255, 255,
            255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 64, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 70,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0,
        ];
        assert!(BamReader::new(&data[..], ()).is_err());

        let data = [
            66, 65, 77, 1, 62, 1, 0, 0, 0, 0, 0, 0, 12, 10, 255, 255, 255, 255, 255, 255, 255, 255,
            255, 122, 255, 255, 255, 255, 138, 138, 138, 138, 138, 138, 138, 138, 138, 138, 138,
            138, 138, 138, 138, 138, 138, 138, 138, 138, 142, 138, 138, 138, 138, 138, 138, 138,
            138, 138, 138, 138, 202, 138, 138, 138, 138, 138, 138, 138, 138, 255, 255, 255, 255,
            255, 255, 255, 255, 255, 10, 10, 20, 10, 10, 10, 10, 62, 0, 0, 0, 0, 0, 0, 0, 0, 0, 74,
            10, 10, 10, 10, 10, 117, 117, 117, 126, 117, 117, 117, 117, 117, 117, 117, 117, 117,
            117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 255, 255, 255, 255, 255,
            255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 253, 255, 255, 255, 255, 255,
            255, 255, 255, 6, 0, 255, 255, 246, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 64, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 10, 200,
            62, 10, 134, 10, 62, 10, 10, 10, 10, 10, 10, 10, 10, 62, 0, 0, 10, 10, 10, 103, 10, 10,
            10, 181, 181, 181, 181, 181, 181, 181, 181, 62, 10, 10, 10, 10, 10, 10, 10, 68, 61, 10,
            10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 107, 181, 181, 181,
            181, 181, 181, 181, 181, 181, 181, 181, 181, 181, 181, 181, 181, 181, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 5, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 96, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            35, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 5, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 10, 10, 10, 62, 10,
        ];
        let mut reader = BamReader::new(&data[..], ())?;
        assert!(reader.next().is_err());

        let data = [
            66, 65, 77, 1, 62, 1, 0, 0, 0, 0, 0, 0, 12, 10, 255, 255, 255, 255, 255, 255, 255, 255,
            255, 122, 255, 255, 255, 255, 138, 138, 138, 138, 138, 138, 138, 138, 138, 138, 138,
            138, 138, 138, 138, 138, 138, 138, 138, 138, 142, 138, 138, 138, 138, 138, 138, 138,
            138, 138, 138, 138, 202, 138, 138, 138, 138, 138, 138, 138, 138, 255, 255, 255, 255,
            255, 255, 255, 255, 223, 10, 10, 20, 10, 10, 10, 10, 62, 0, 0, 0, 0, 0, 0, 0, 0, 0, 74,
            10, 10, 10, 10, 10, 117, 117, 117, 126, 117, 117, 117, 117, 117, 117, 117, 117, 117,
            117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 255, 255, 255, 255, 255,
            255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 253, 255, 255, 255, 255, 255,
            255, 255, 255, 6, 0, 255, 255, 246, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 64, 0, 0, 0, 0, 255, 255, 255, 255, 72, 97, 112, 115, 71, 80, 73, 82, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 10, 200, 62, 10, 134, 10, 62, 10, 10, 10, 10, 10, 10, 10, 10, 62, 0, 0, 10,
            10, 10, 103, 10, 10, 10, 181, 181, 181, 181, 181, 181, 181, 181, 62, 10, 10, 10, 10,
            10, 10, 61, 10, 68, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10,
            10, 107, 181, 181, 181, 181, 181, 181, 181, 181, 181, 181, 181, 181, 181, 181, 181,
            181, 181, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 5, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 96, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 35, 0, 0, 0, 0, 0, 0, 0, 0, 31, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 254, 251, 0, 0, 0, 0, 0, 0, 6, 0, 0, 0,
            10, 10, 10, 62, 10,
        ];
        let mut reader = BamReader::new(&data[..], ())?;
        assert!(reader.next().is_ok());
        assert!(reader.next().is_err());

        let data = [
            66, 65, 77, 1, 62, 1, 0, 0, 0, 0, 0, 0, 12, 10, 255, 255, 255, 255, 255, 255, 255, 255,
            255, 122, 255, 255, 255, 255, 138, 138, 138, 138, 138, 138, 138, 138, 138, 138, 138,
            138, 138, 138, 138, 138, 138, 138, 138, 138, 142, 138, 138, 138, 138, 138, 138, 138,
            138, 138, 138, 138, 202, 138, 138, 138, 138, 138, 138, 138, 138, 255, 255, 255, 255,
            255, 255, 255, 255, 255, 10, 10, 20, 10, 10, 10, 10, 62, 0, 0, 0, 0, 0, 0, 0, 0, 0, 74,
            10, 10, 10, 10, 10, 117, 117, 117, 126, 117, 117, 117, 117, 117, 117, 117, 117, 117,
            117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 255, 255, 255, 255, 255,
            255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 253, 255, 255, 255, 255, 255,
            255, 255, 255, 6, 0, 255, 255, 246, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 64, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 10, 200,
            62, 10, 134, 10, 62, 10, 10, 10, 10, 10, 10, 10, 10, 62, 0, 0, 10, 10, 10, 103, 10, 10,
            10, 181, 181, 181, 181, 181, 181, 181, 181, 62, 10, 10, 10, 10, 10, 10, 10, 68, 61, 10,
            10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 107, 181, 181, 181,
            181, 181, 181, 181, 181, 181, 181, 181, 181, 181, 181, 181, 181, 181, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 5, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 96, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            35, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 4, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 10, 10, 10, 10, 62, 10,
        ];
        let mut reader = BamReader::new(&data[..], ())?;
        assert!(reader.next().is_err());

        let data = [
            66, 65, 77, 1, 62, 1, 0, 0, 0, 0, 0, 0, 12, 10, 255, 255, 255, 255, 255, 255, 255, 255,
            255, 122, 255, 255, 255, 255, 138, 138, 138, 138, 138, 138, 138, 138, 138, 138, 138,
            138, 138, 138, 138, 138, 138, 138, 138, 138, 142, 138, 138, 138, 138, 138, 138, 138,
            138, 138, 138, 138, 202, 138, 138, 138, 138, 138, 138, 138, 138, 255, 255, 255, 255,
            255, 255, 255, 255, 255, 10, 10, 20, 10, 10, 10, 10, 62, 0, 0, 0, 0, 0, 0, 0, 0, 0, 74,
            10, 10, 10, 10, 10, 117, 117, 117, 126, 117, 117, 117, 117, 117, 117, 117, 117, 117,
            117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 255, 255, 255, 255, 255,
            255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 253, 255, 255, 255, 255, 255,
            255, 255, 255, 6, 0, 255, 255, 246, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 64, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 10, 200,
            62, 10, 134, 10, 62, 10, 10, 10, 10, 10, 10, 10, 10, 62, 0, 0, 10, 10, 10, 103, 10, 10,
            10, 181, 181, 181, 181, 181, 181, 181, 181, 62, 10, 10, 10, 10, 10, 10, 10, 68, 61, 10,
            10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 120, 10, 10, 107, 181, 181,
            181, 181, 181, 181, 181, 181, 181, 181, 181, 181, 181, 181, 181, 181, 181, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 5, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 96, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 35, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            43, 10, 8, 64, 0, 0, 0, 6, 0, 255, 255, 246, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255,
            255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
            255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            8, 1, 0, 0, 0, 0, 0, 0, 201, 64, 248, 10, 62, 44, 10, 255, 255, 255, 255, 255, 0, 0, 0,
            0, 4, 3, 2, 1, 83, 80, 65, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255,
            255, 255, 175, 255, 255, 255, 10, 10, 62, 0, 13, 10, 10, 220, 227, 10, 10, 97, 62, 0,
            13, 10, 10, 227, 10, 10, 62, 10, 59, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 255, 255, 255, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 15, 230, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 38, 0, 0, 0, 0, 0,
            0, 0, 0, 248, 248, 248, 248, 248, 248, 248, 248, 248, 248, 248, 248, 248, 248, 248, 0,
            0, 10, 10, 10, 10, 62, 10,
        ];
        let mut reader = BamReader::new(&data[..], ())?;
        assert!(reader.next().is_ok());
        assert!(reader.next().is_err());

        let data = [
            66, 65, 77, 1, 62, 1, 0, 0, 0, 0, 0, 0, 12, 10, 255, 255, 255, 255, 255, 255, 255, 255,
            255, 122, 255, 255, 255, 255, 138, 138, 138, 138, 138, 138, 138, 138, 138, 138, 138,
            138, 138, 138, 138, 138, 138, 138, 138, 138, 142, 138, 138, 138, 138, 138, 138, 138,
            138, 138, 138, 138, 202, 138, 138, 138, 138, 138, 138, 138, 138, 255, 255, 255, 255,
            10, 10, 20, 10, 10, 10, 10, 62, 0, 0, 0, 0, 0, 0, 0, 0, 0, 74, 10, 10, 10, 10, 10, 117,
            117, 117, 126, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117,
            117, 117, 117, 117, 117, 117, 117, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
            255, 255, 255, 255, 255, 255, 253, 255, 255, 255, 255, 255, 255, 255, 255, 6, 0, 255,
            255, 246, 0, 0, 0, 0, 0, 0, 0, 0, 129, 0, 16, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 64, 0,
            0, 0, 0, 255, 255, 255, 255, 72, 97, 112, 115, 71, 80, 73, 82, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 10, 200, 62,
            10, 134, 10, 62, 10, 10, 10, 10, 10, 10, 10, 10, 62, 0, 0, 10, 10, 10, 103, 10, 10, 10,
            181, 181, 181, 181, 181, 181, 181, 181, 62, 10, 10, 10, 10, 10, 10, 10, 68, 61, 10, 10,
            10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 107, 181, 181, 181,
            181, 181, 181, 181, 181, 181, 181, 181, 181, 181, 181, 181, 181, 181, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 5, 0, 0, 0, 0, 5, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 96, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 35, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 130, 130, 3, 0, 0,
            0, 47, 0, 255, 254, 255, 255, 255, 255, 255, 255, 255, 255, 255, 0, 10,
        ];
        let mut reader = BamReader::new(&data[..], ())?;
        assert!(reader.next().is_err());

        let data = [
            66, 65, 77, 1, 62, 1, 0, 0, 0, 0, 0, 0, 12, 10, 255, 255, 255, 255, 255, 255, 255, 255,
            255, 122, 255, 255, 255, 255, 138, 138, 138, 138, 138, 138, 138, 138, 138, 138, 138,
            138, 138, 138, 138, 138, 138, 138, 138, 138, 142, 138, 138, 138, 138, 138, 138, 138,
            138, 138, 138, 138, 202, 138, 138, 138, 138, 138, 138, 138, 138, 255, 255, 255, 255,
            255, 255, 255, 255, 255, 10, 10, 20, 10, 10, 10, 10, 62, 0, 0, 0, 0, 0, 0, 0, 0, 0, 74,
            10, 10, 10, 10, 10, 117, 117, 117, 126, 117, 117, 117, 117, 117, 117, 117, 117, 117,
            117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 117, 255, 255, 255, 255, 255,
            255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 253, 255, 255, 255, 255, 255,
            255, 255, 255, 6, 0, 255, 255, 246, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 64, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 10, 200,
            62, 10, 134, 10, 62, 10, 10, 10, 10, 10, 10, 10, 10, 62, 0, 0, 10, 10, 10, 103, 10, 10,
            10, 181, 181, 181, 181, 181, 181, 181, 181, 62, 10, 10, 10, 10, 10, 10, 10, 68, 61, 10,
            10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 107, 181, 181, 181,
            181, 181, 181, 181, 181, 181, 181, 181, 181, 181, 181, 181, 181, 181, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 5, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 96, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            33, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            43, 10, 8, 64, 0, 0, 0, 6, 0, 0, 201, 64, 248, 10, 62, 44, 10, 255, 255, 255, 10, 255,
            255, 0, 0, 0, 0, 4, 3, 2, 1, 83, 80, 65, 72, 66, 65, 77, 0, 0, 0, 62, 10, 0, 0, 0, 0,
            0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 175, 255, 255, 255, 10, 10, 10,
            62, 0, 13, 10, 10, 220, 227, 10, 10, 97, 62, 0, 13, 10, 10, 227, 10, 10, 62, 10, 59,
            10, 10, 10, 10,
        ];
        let mut reader = BamReader::new(&data[..], ())?;
        assert!(reader.next().is_err());

        Ok(())
    }
}
