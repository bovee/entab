use alloc::boxed::Box;

use crate::record::ReaderBuilder;

pub mod chemstation;
pub mod fasta;
pub mod fastq;
pub mod tsv;

pub fn get_builder(parser_type: &str) -> Option<Box<dyn ReaderBuilder>> {
    Some(match parser_type {
        "chemstation" => Box::new(chemstation::ChemstationMsReaderBuilder::default()),
        "fasta" => Box::new(fasta::FastaReaderBuilder::default()),
        "fastq" => Box::new(fastq::FastqReaderBuilder::default()),
        "tsv" => Box::new(tsv::TsvReaderBuilder::default()),
        _ => return None,
    })
}
