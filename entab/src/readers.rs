use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::convert::TryInto;

use crate::buffer::ReadBuffer;
use crate::error::EtError;
use crate::filetype::FileType;
use crate::parsers;
use crate::record::Value;

/// Turn `rb` into a Reader of type `parser_type`
pub fn get_reader<'r, B>(
    file_type: FileType,
    data: B,
) -> Result<Box<dyn RecordReader + 'r>, EtError>
where
    B: TryInto<ReadBuffer<'r>>,
    EtError: From<<B as TryInto<ReadBuffer<'r>>>::Error>,
{
    Ok(match file_type {
        FileType::Bam => Box::new(parsers::sam::BamReader::new(data, ())?),
        FileType::AgilentChemstationFid => {
            Box::new(parsers::agilent::chemstation::ChemstationFidReader::new(data, ())?)
        }
        FileType::AgilentChemstationMs => {
            Box::new(parsers::agilent::chemstation::ChemstationMsReader::new(data, ())?)
        }
        FileType::AgilentChemstationMwd => {
            Box::new(parsers::agilent::chemstation::ChemstationMwdReader::new(data, ())?)
        }
        FileType::AgilentChemstationUv => {
            Box::new(parsers::agilent::chemstation::ChemstationUvReader::new(data, ())?)
        }
        FileType::Fasta => Box::new(parsers::fasta::FastaReader::new(data, ())?),
        FileType::Fastq => Box::new(parsers::fastq::FastqReader::new(data, ())?),
        FileType::Facs => Box::new(parsers::flow::FcsReader::new(data, ())?),
        FileType::InficonHapsite => {
            Box::new(parsers::inficon::InficonReader::new(data, (Vec::new(), 0))?)
        }
        #[cfg(feature = "std")]
        FileType::Png => Box::new(parsers::png::PngReader::new(data, ())?),
        FileType::Sam => Box::new(parsers::sam::SamReader::new(data, ())?),
        FileType::ThermoCf => Box::new(parsers::thermo::thermo_iso::ThermoCfReader::new(data, ())?),
        FileType::ThermoDxf => {
            Box::new(parsers::thermo::thermo_iso::ThermoDxfReader::new(data, ())?)
        }
        FileType::DelimitedText(d) => Box::new(parsers::tsv::TsvReader::new(data, (d, b'"'))?),
        _ => return Err(format!("No parser available for the filetype {:?}", file_type).into()),
    })
}

/// The trait that maps over "generic" `RecordReader`s
///
/// Structs that implement this trait should also implement a `new` method that
/// takes a `ReadBuffer` and a "state" for creation and a `next` method that
/// returns a "specialized" struct that can be turned into the "generic" struct
/// via the `next_record` method.
pub trait RecordReader: ::core::fmt::Debug {
    /// Returns the next record from the file.
    ///
    /// Roughly equivalent to Rust's `Iterator.next`, but obeys slightly
    /// looser lifetime requirements to allow zero-copy parsing.
    fn next_record(&mut self) -> Result<Option<Vec<Value>>, EtError>;

    /// The header titles that correspond to every item in the record
    fn headers(&self) -> Vec<String>;

    /// Extra metadata about the file or data in the file
    fn metadata(&self) -> BTreeMap<String, Value>;
}

/// Generates a `...Reader` struct for the associated state-based file parsers
/// along with the matching `RecordReader` for that struct.
#[macro_export]
macro_rules! impl_reader {
    ($(#[$attr:meta])* $reader: ident, $record:ty, $state:ty, $new_params:ty) => {
        $(#[$attr])*
        /// [this reader was autogenerated via macro]
        #[derive(Debug)]
        pub struct $reader<'r> {
            rb: $crate::buffer::ReadBuffer<'r>,
            state: $state,
        }

        impl<'r> $reader<'r> {
            /// Create a new instance of the reader
            ///
            /// # Errors
            /// If data could not be turned into a `ReadBuffer` successfully or if the initial state
            /// could not be extracted, returns an `EtError`.
            pub fn new<B>(data: B, params: $new_params) -> Result<Self, EtError> where
                B: ::core::convert::TryInto<$crate::buffer::ReadBuffer<'r>>,
                EtError: From<<B as ::core::convert::TryInto<$crate::buffer::ReadBuffer<'r>>>::Error>,
            {
                let mut rb = data.try_into()?;
                match rb.next(params)? {
                    Some(state) => Ok($reader { rb, state }),
                    None => Err(::alloc::format!("Could not initialize state {}", ::core::any::type_name::<$state>()) .into())
                }
            }

            /// Return the specialized version of this record.
            ///
            /// To get the "generic" version, please use the `next_record`
            /// method from the `RecordReader` trait.
            ///
            /// # Errors
            /// If a value could not be extracted, return an `EtError`.
            #[allow(clippy::should_implement_trait)]
            pub fn next(&mut self) -> Result<Option<$record>, EtError> {
                self.rb.next::<$record>(&mut self.state)
            }
        }

        impl<'r> $crate::readers::RecordReader for $reader<'r> {
            /// The next record, expressed as a `Vec` of `Value`s.
            fn next_record(
                &mut self,
            ) -> Result<Option<::alloc::vec::Vec<$crate::record::Value>>, EtError> {
                if let Some(record) = self.rb.next::<$record>(&mut self.state)? {
                    Ok(Some(record.into()))
                } else {
                    Ok(None)
                }
            }

            /// The headers for this Reader.
            fn headers(&self) -> ::alloc::vec::Vec<::alloc::string::String> {
                use $crate::record::StateMetadata;
                use ::alloc::string::ToString;
                self.state.header().iter().map(|s| s.to_string()).collect()
            }

            /// The metadata for this Reader.
            fn metadata(&self) -> ::alloc::collections::BTreeMap<::alloc::string::String, $crate::record::Value> {
                use $crate::record::StateMetadata;
                self.state.metadata()
            }
        }
    };
}