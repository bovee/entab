use alloc::boxed::Box;

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

pub fn get_reader<'r>(
    parser_type: &str,
    rb: ReadBuffer<'r>,
) -> Result<Box<dyn RecordReader + 'r>, EtError> {
    Ok(match parser_type {
        "bam" => Box::new(sam::BamReader::new(rb)?),
        "cf" => Box::new(thermo_iso::ThermoCfReader::new(rb)?),
        "chemstation_fid" => Box::new(chemstation::ChemstationFidReader::new(rb)?),
        "chemstation_ms" => Box::new(chemstation::ChemstationMsReader::new(rb)?),
        "chemstation_mwd" => Box::new(chemstation::ChemstationMwdReader::new(rb)?),
        "chemstation_uv" => Box::new(chemstation::ChemstationUvReader::new(rb)?),
        "dxf" => Box::new(thermo_iso::ThermoDxfReader::new(rb)?),
        "fasta" => Box::new(fasta::FastaReader::new(rb)?),
        "fastq" => Box::new(fastq::FastqReader::new(rb)?),
        "sam" => Box::new(sam::SamReader::new(rb)?),
        "tsv" => Box::new(tsv::TsvReader::new(rb, b'\t', b'"')?),
        _ => {
            return Err(EtError::new(format!(
                "No parser available for the filetype {}",
                parser_type
            )))
        }
    })
}

pub trait RecordReader {
    /// Returns the next record from the file.
    ///
    /// Roughly equivalent to Rust's `Iterator.next`, but obeys slightly
    /// looser lifetime requirements to allow zero-copy parsing.
    fn next(&mut self) -> Result<Option<Record>, EtError>;

    // TODO: add a metadata retrieval method that returns a toml::value::Value or
    // erased_serde::Serialize or something else generic like that
}

#[macro_export]
macro_rules! impl_reader {
    ($reader: ident, $state:ty, $record:ty) => {
        pub struct $reader<'r> {
            rb: ReadBuffer<'r>,
            state: $state,
        }

        impl<'r> $reader<'r> {
            pub fn new(mut rb: ReadBuffer<'r>) -> Result<Self, EtError> {
                let state = rb.extract(())?;
                Ok($reader { rb, state })
            }
        }

        impl<'r> crate::readers::RecordReader for $reader<'r> {
            fn next(&mut self) -> Result<Option<Record>, EtError> {
                if let Some(record) = self.rb.extract::<Option<$record>>(&mut self.state)? {
                    Ok(Some(record.into()))
                } else {
                    Ok(None)
                }
            }
        }
    };
}
