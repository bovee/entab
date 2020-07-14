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
pub mod thermo;
pub mod tsv;

pub fn get_builder(parser_type: &str) -> Option<Box<dyn ReaderBuilder>> {
    Some(match parser_type {
        "bam" => Box::new(sam::BamReaderBuilder::default()),
        "chemstation" => Box::new(chemstation::ChemstationMsReaderBuilder::default()),
        "dxf" => Box::new(thermo::ThermoDxfReaderBuilder::default()),
        "fasta" => Box::new(fasta::FastaReaderBuilder::default()),
        "fastq" => Box::new(fastq::FastqReaderBuilder::default()),
        "sam" => Box::new(sam::SamReaderBuilder::default()),
        "tsv" => Box::new(tsv::TsvReaderBuilder::default()),
        _ => return None,
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
