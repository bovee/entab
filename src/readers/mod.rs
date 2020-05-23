pub mod chemstation;
pub mod fasta;
pub mod fastq;
pub mod tsv;

/// This helps generate the bindings from each parser type to the parser builders themselves to
/// allow calling functions like `handle_buffer<R, ..>(..) where R: ReaderBuilder, ..`.
#[macro_export]
macro_rules! all_types {
    (match $m:expr => $f:ident::<$($t:ty)*>($($arg:expr),*)) => {
        match $m {
            "chemstation" => $f::<$crate::readers::chemstation::ChemstationMsReaderBuilder,$($t),*>($($arg),*),
            "fasta" => $f::<$crate::readers::fasta::FastaReaderBuilder,$($t),*>($($arg),*),
            "fastq" => $f::<$crate::readers::fastq::FastqReaderBuilder,$($t),*>($($arg),*),
            "tsv" => $f::<$crate::readers::tsv::TsvReaderBuilder,$($t),*>($($arg),*),
            _ => Err(EtError::new("No parser found for file type")),
        }
    };
}
