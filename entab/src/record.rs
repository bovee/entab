use alloc::borrow::Cow;
use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};

use serde::Serialize;

use crate::buffer::ReadBuffer;
use crate::utils::string::replace_tabs;
use crate::EtError;

#[derive(Debug, Serialize)]
pub enum Record<'r> {
    Mz {
        time: f64,
        mz: f64,
        intensity: u64,
    },
    MzFloat {
        time: f64,
        mz: f64,
        intensity: f64,
    },
    Sam {
        query_name: &'r str,
        flag: u16,
        ref_name: &'r str,
        pos: Option<u64>,
        mapq: Option<u8>,
        cigar: Cow<'r, [u8]>,
        rnext: &'r str,
        pnext: Option<u32>,
        tlen: i32,
        seq: Cow<'r, [u8]>,
        qual: Cow<'r, [u8]>,
        extra: Cow<'r, [u8]>,
    },
    Sequence {
        id: &'r str,
        sequence: Cow<'r, [u8]>,
        quality: Option<&'r [u8]>,
        // TODO: a kmer position or offset?
    },
    Tsv(&'r [&'r str], &'r [String]),
}

impl<'r> Record<'r> {
    pub fn headers(&self) -> Cow<[&str]> {
        match self {
            Self::Mz { .. } => Cow::Borrowed(&["time", "mz", "intensity"]),
            Self::MzFloat { .. } => Cow::Borrowed(&["time", "mz", "intensity"]),
            Self::Sequence { .. } => Cow::Borrowed(&["id", "sequence", "quality"]),
            Self::Sam { .. } => Cow::Borrowed(&[
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
            ]),
            Self::Tsv(_, headers) => Cow::Owned(headers.iter().map(|i| i.as_ref()).collect()),
        }
    }

    pub fn size(&self) -> usize {
        match self {
            Self::Mz { .. } => 3,
            Self::MzFloat { .. } => 3,
            Self::Sequence { .. } => 3,
            Self::Sam { .. } => 12,
            Self::Tsv(rec, _) => rec.len(),
        }
    }

    /// Writes a single field of the Record out.
    ///
    /// Note: W is not the Write trait to keep this no_std compatible.
    pub fn write_field<W>(&self, index: usize, mut write: W) -> Result<(), EtError>
    where
        W: FnMut(&[u8]) -> Result<(), EtError>,
    {
        match (self, index) {
            (Self::Mz { time, .. }, 0) => write(format!("{:02}", time).as_bytes())?,
            (Self::Mz { mz, .. }, 1) => write(format!("{:02}", mz).as_bytes())?,
            (Self::Mz { intensity, .. }, 2) => write(intensity.to_string().as_bytes())?,
            (Self::MzFloat { time, .. }, 0) => write(format!("{:02}", time).as_bytes())?,
            (Self::MzFloat { mz, .. }, 1) => write(format!("{:02}", mz).as_bytes())?,
            (Self::MzFloat { intensity, .. }, 2) => write(format!("{:02}", intensity).as_bytes())?,
            (Self::Sam { query_name, .. }, 0) => write(&replace_tabs(query_name.as_bytes(), b'|'))?,
            // TODO: better display for flags?
            (Self::Sam { flag, .. }, 1) => write(format!("{:b}", flag).as_bytes())?,
            (Self::Sam { ref_name, .. }, 2) => write(&replace_tabs(ref_name.as_bytes(), b'|'))?,
            (Self::Sam { pos, .. }, 3) => {
                if let Some(p) = pos {
                    write(p.to_string().as_bytes())?
                };
            }
            (Self::Sam { mapq, .. }, 4) => {
                if let Some(m) = mapq {
                    write(m.to_string().as_bytes())?
                };
            }
            (Self::Sam { cigar, .. }, 5) => write(cigar)?,
            (Self::Sam { rnext, .. }, 6) => write(&replace_tabs(rnext.as_bytes(), b'|'))?,
            (Self::Sam { pnext, .. }, 7) => {
                if let Some(p) = pnext {
                    write(p.to_string().as_bytes())?
                };
            }
            (Self::Sam { tlen, .. }, 8) => write(tlen.to_string().as_bytes())?,
            (Self::Sam { seq, .. }, 9) => write(seq)?,
            (Self::Sam { qual, .. }, 10) => write(qual)?,
            (Self::Sam { extra, .. }, 11) => write(&replace_tabs(extra, b'|'))?,
            (Self::Sequence { id, .. }, 0) => write(&replace_tabs(id.as_bytes(), b'|'))?,
            (Self::Sequence { sequence, .. }, 1) => write(sequence)?,
            (Self::Sequence { quality, .. }, 2) => {
                if let Some(q) = quality {
                    write(q)?
                };
            }
            (Self::Tsv(rec, _), i) => write(rec[i].as_bytes())?,
            _ => panic!("Index out of range"),
        }
        Ok(())
    }
    // fn get(&self, field: &str) -> Option<Value>;
}

pub trait ReaderBuilder {
    fn to_reader<'r>(&self, rb: ReadBuffer<'r>) -> Result<Box<dyn RecordReader + 'r>, EtError>;
}

pub trait RecordReader {
    fn next(&mut self) -> Result<Option<Record>, EtError>;
}
