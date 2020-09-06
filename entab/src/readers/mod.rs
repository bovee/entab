use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::buffer::ReadBuffer;
use crate::record::Value;
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
        "bam" => Box::new(sam::BamReader::new(rb, ())?),
        "chemstation_fid" => Box::new(chemstation::ChemstationFidReader::new(rb, ())?),
        "chemstation_ms" => Box::new(chemstation::ChemstationMsReader::new(rb, ())?),
        "chemstation_mwd" => Box::new(chemstation::ChemstationMwdReader::new(rb, ())?),
        "chemstation_uv" => Box::new(chemstation::ChemstationUvReader::new(rb, ())?),
        "fasta" => Box::new(fasta::FastaReader::new(rb, ())?),
        "fastq" => Box::new(fastq::FastqReader::new(rb, ())?),
        "sam" => Box::new(sam::SamReader::new(rb, ())?),
        "thermo_cf" => Box::new(thermo_iso::ThermoCfReader::new(rb, ())?),
        "thermo_dxf" => Box::new(thermo_iso::ThermoDxfReader::new(rb, ())?),
        "tsv" => Box::new(tsv::TsvReader::new(rb, (b'\t', b'"'))?),
        _ => {
            return Err(EtError::new(format!(
                "No parser available for the filetype {}",
                parser_type
            )))
        }
    })
}

pub trait RecordReader: ::core::fmt::Debug {
    /// Returns the next record from the file.
    ///
    /// Roughly equivalent to Rust's `Iterator.next`, but obeys slightly
    /// looser lifetime requirements to allow zero-copy parsing.
    fn next_record(&mut self) -> Result<Option<Vec<Value>>, EtError>;

    /// The header titles that correspond to every item in the record
    fn headers(&self) -> Vec<String>;
}

// TODO: we need to return metadata too somehow

#[macro_export]
macro_rules! impl_reader {
    ($reader: ident, $record:ty, $state:ty, $new_params:ty) => {
        #[derive(Debug)]
        pub struct $reader<'r> {
            rb: ReadBuffer<'r>,
            state: $state,
        }

        impl<'r> $reader<'r> {
            pub fn new(mut rb: ReadBuffer<'r>, params: $new_params) -> Result<Self, EtError> {
                let state = rb.extract(params)?;
                Ok($reader { rb, state })
            }

            pub fn next(&mut self) -> Result<Option<$record>, EtError> {
                self.rb.record_pos += 1;
                self.rb.extract(&mut self.state)
            }
        }

        impl<'r> crate::readers::RecordReader for $reader<'r> {
            fn next_record(
                &mut self,
            ) -> Result<Option<::alloc::vec::Vec<$crate::record::Value>>, EtError> {
                if let Some(record) = self.rb.extract::<Option<$record>>(&mut self.state)? {
                    Ok(Some(record.into()))
                } else {
                    Ok(None)
                }
            }

            fn headers(&self) -> ::alloc::vec::Vec<::alloc::string::String> {
                use $crate::record::RecHeader;
                <$record>::header()
            }
        }
    };
}
