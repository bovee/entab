use alloc::borrow::Cow;
use alloc::boxed::Box;
use alloc::vec::Vec;

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
    Fasta {
        id: &'r str,
        sequence: Cow<'r, [u8]>,
    },
    Fastq {
        id: &'r str,
        sequence: &'r [u8],
        quality: &'r [u8],
    },
    Tsv(&'r [&'r str]),
}

impl<'r> Record<'r> {
    pub fn size(&self) -> usize {
        match self {
            Self::Mz { .. } => 3,
            Self::Fasta { .. } => 2,
            Self::Fastq { .. } => 3,
            Self::Tsv(rec) => rec.len(),
        }
    }

    pub fn write_field<W>(&self, index: usize, mut write: W) -> Result<(), EtError>
    where
        W: FnMut(&[u8]) -> Result<(), EtError>,
    {
        match (self, index) {
            (Self::Mz { time, .. }, 0) => write(format!("{:02}", time).as_bytes())?,
            (Self::Mz { mz, .. }, 1) => write(format!("{:02}", mz).as_bytes())?,
            (Self::Mz { intensity, .. }, 2) => write(format!("{:02}", intensity).as_bytes())?,
            (Self::Fasta { id, .. }, 0) => write(&replace_tabs(id.as_bytes(), b'|'))?,
            (Self::Fasta { sequence, .. }, 1) => write(sequence.as_ref())?,
            (Self::Fastq { id, .. }, 0) => write(&replace_tabs(id.as_bytes(), b'|'))?,
            (Self::Fastq { sequence, .. }, 1) => write(sequence)?,
            (Self::Fastq { quality, .. }, 2) => write(quality)?,
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
