use alloc::boxed::Box;
#[cfg(feature = "std")]
use std::io::Read;

use crate::buffer::ReadBuffer;
use crate::record::Record;
use crate::utils::error::EtError;

pub mod chemstation;
pub mod fasta;
pub mod fastq;
pub mod kmers;
pub mod sam;
pub mod thermo_iso;
pub mod tsv;

pub fn get_reader<'r>(parser_type: &str, rb: ReadBuffer<'r>) -> Result<Box<dyn RecordReader + 'r>, EtError> {
    Ok(match parser_type {
        "bam" => Box::new(sam::BamReader::new(rb)?),
        "cf" => Box::new(thermo_iso::ThermoCfReader::new(rb)?),
        "chemstation" => Box::new(chemstation::ChemstationMsReader::new(rb)?),
        "dxf" => Box::new(thermo_iso::ThermoDxfReader::new(rb)?),
        "fasta" => Box::new(fasta::FastaReader::new(rb)?),
        "fastq" => Box::new(fastq::FastqReader::new(rb)?),
        "sam" => Box::new(sam::SamReader::new(rb)?),
        "tsv" => Box::new(tsv::TsvReader::new(rb, b'\t', b'"')?),
        _ => return Err(EtError::new("No parser available for the filetype determine")),
    })
}


pub trait ReaderBuilder {
    fn to_reader<'r>(&self, rb: ReadBuffer<'r>) -> Result<Box<dyn RecordReader + 'r>, EtError>;

    /// A wrapper around `to_reader` to reduce the boilerplate of creating a
    /// `ReadBuffer` before calling it.
    #[cfg(feature = "std")]
    fn from_stream<'r>(
        &self,
        stream: Box<dyn Read + 'r>,
    ) -> Result<Box<dyn RecordReader + 'r>, EtError> {
        let rb = ReadBuffer::new(stream)?;
        self.to_reader(rb)
    }
}

pub trait RecordReader {
    /// Returns the next record from the file.
    ///
    /// Roughly equivalent to Rust's `Iterator.next`, but obeys slightly
    /// looser lifetime requirements to allow zero-copy parsing.
    fn next(&mut self) -> Result<Option<Record>, EtError>;
}
