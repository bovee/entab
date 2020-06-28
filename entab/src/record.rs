use alloc::borrow::Cow;
use alloc::boxed::Box;
use alloc::format;
use alloc::vec::Vec;

use serde::Serialize;

use crate::buffer::ReadBuffer;
use crate::utils::string::replace_tabs;
use crate::EtError;

#[derive(Debug, Serialize)]
pub enum Record<'r> {
    Fasta {
        id: &'r str,
        sequence: Cow<'r, [u8]>,
    },
    Fastq {
        id: &'r str,
        sequence: &'r [u8],
        quality: &'r [u8],
    },
    Kmer {
        id: Cow<'r, str>,
        kmer: Cow<'r, [u8]>,
        sequence_index: usize,
        kmer_index: usize,
    },
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
    Raw(&'r [u8]),
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
        qual: &'r [u8],
        extra: Cow<'r, [u8]>,
    },
    Tsv(&'r [&'r str]),
}

impl<'r> Record<'r> {
    pub fn size(&self) -> usize {
        match self {
            Self::Mz { .. } => 3,
            Self::MzFloat { .. } => 3,
            Self::Kmer { .. } => 4,
            Self::Fasta { .. } => 2,
            Self::Fastq { .. } => 3,
            Self::Raw { .. } => 1,
            Self::Sam { .. } => 12,
            Self::Tsv(rec) => rec.len(),
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
            (Self::Fasta { id, .. }, 0) => write(&replace_tabs(id.as_bytes(), b'|'))?,
            (Self::Fasta { sequence, .. }, 1) => write(sequence.as_ref())?,
            (Self::Fastq { id, .. }, 0) => write(&replace_tabs(id.as_bytes(), b'|'))?,
            (Self::Fastq { sequence, .. }, 1) => write(sequence)?,
            (Self::Fastq { quality, .. }, 2) => write(quality)?,
            (Self::Kmer { id, .. }, 0) => write(&replace_tabs(id.as_bytes(), b'|'))?,
            (Self::Kmer { kmer, .. }, 1) => write(kmer)?,
            (Self::Kmer { sequence_index, .. }, 2) => write(sequence_index.to_string().as_bytes())?,
            (Self::Kmer { kmer_index, .. }, 3) => write(kmer_index.to_string().as_bytes())?,
            (Self::Mz { time, .. }, 0) => write(format!("{:02}", time).as_bytes())?,
            (Self::Mz { mz, .. }, 1) => write(format!("{:02}", mz).as_bytes())?,
            (Self::Mz { intensity, .. }, 2) => write(intensity.to_string().as_bytes())?,
            (Self::MzFloat { time, .. }, 0) => write(format!("{:02}", time).as_bytes())?,
            (Self::MzFloat { mz, .. }, 1) => write(format!("{:02}", mz).as_bytes())?,
            (Self::MzFloat { intensity, .. }, 2) => write(format!("{:02}", intensity).as_bytes())?,
            (Self::Raw(b), 0) => write(b)?,
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
            (Self::Tsv(rec), i) => write(rec[i].as_bytes())?,
            _ => panic!("Index out of range"),
        }
        Ok(())
    }
    // fn get(&self, field: &str) -> Option<Value>;
}

pub trait BindT<'b> {
    type Assoc;
}

pub struct RecordT;
impl<'b> BindT<'b> for RecordT {
    type Assoc = Record<'b>;
}

pub trait ReaderBuilder {
    fn to_reader<'r>(&self, rb: ReadBuffer<'r>) -> Result<Box<dyn RecordReader + 'r>, EtError>;
}

pub trait RecordReader {
    fn headers(&self) -> Vec<&str>;
    fn next(&mut self) -> Result<Option<Record>, EtError>;
}
