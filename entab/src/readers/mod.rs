use alloc::boxed::Box;

use crate::record::ReaderBuilder;

pub mod chemstation;
pub mod fasta;
pub mod fastq;
pub mod kmers;
pub mod sam;
pub mod tsv;

pub fn get_builder(parser_type: &str) -> Option<Box<dyn ReaderBuilder>> {
    Some(match parser_type {
        "bam" => Box::new(sam::BamReaderBuilder::default()),
        "chemstation" => Box::new(chemstation::ChemstationMsReaderBuilder::default()),
        "fasta" => Box::new(fasta::FastaReaderBuilder::default()),
        "fastq" => Box::new(fastq::FastqReaderBuilder::default()),
        "sam" => Box::new(sam::SamReaderBuilder::default()),
        "tsv" => Box::new(tsv::TsvReaderBuilder::default()),
        _ => return None,
    })
}
