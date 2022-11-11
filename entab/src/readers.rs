use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::convert::TryInto;

use crate::buffer::ReadBuffer;
use crate::compression::decompress;
use crate::error::EtError;
use crate::parsers;
use crate::parsers::FromSlice;
use crate::record::Value;

/// Turn `rb` into a Reader of type `parser`.
///
/// If `parser` is `None`, infer the correct parser from the file type.
///
/// # Errors
/// If an error happens during decompression or parser detection, an `EtError` is returned.
pub fn get_reader<'n, 'p, 'r, B>(
    data: B,
    parser: Option<&'n str>,
    params: Option<BTreeMap<String, Value<'p>>>,
) -> Result<(Box<dyn RecordReader + 'r>, &'n str), EtError>
where
    B: TryInto<ReadBuffer<'r>>,
    EtError: From<<B as TryInto<ReadBuffer<'r>>>::Error>,
{
    let (mut rb, _): (ReadBuffer<'r>, _) = decompress(data)?;
    let parser_name = rb.sniff_filetype()?.to_parser_name(parser)?;
    _get_reader(rb, parser_name, params.unwrap_or_default())
}

/// Internal function to handle `get_reader` not inferring that the Reader constructors need to be
/// created using `ReadBuffer` and not `B`.
fn _get_reader<'n, 'p, 'r>(
    rb: ReadBuffer<'r>,
    parser_name: &'n str,
    mut params: BTreeMap<String, Value<'p>>,
) -> Result<(Box<dyn RecordReader + 'r>, &'n str), EtError> {
    let reader: Box<dyn RecordReader + 'r> = match parser_name {
        "bam" => Box::new(parsers::sam::BamReader::new(rb, None)?),
        "chemstation_dad" => Box::new(parsers::agilent::chemstation::ChemstationDadReader::new(
            rb, None,
        )?),
        "chemstation_fid" => Box::new(parsers::agilent::chemstation::ChemstationFidReader::new(
            rb, None,
        )?),
        "chemstation_ms" => Box::new(parsers::agilent::chemstation::ChemstationMsReader::new(
            rb, None,
        )?),
        "chemstation_mwd" => Box::new(parsers::agilent::chemstation::ChemstationMwdReader::new(
            rb, None,
        )?),
        "chemstation_new_fid" => {
            Box::new(parsers::agilent::chemstation_new::ChemstationNewFidReader::new(rb, None)?)
        }
        "chemstation_new_uv" => {
            Box::new(parsers::agilent::chemstation_new::ChemstationNewUvReader::new(rb, None)?)
        }
        "csv" => Box::new(parsers::tsv::TsvReader::new(
            rb,
            Some(parsers::tsv::TsvParams::default().delim(b',')),
        )?),
        "fasta" => Box::new(parsers::fasta::FastaReader::new(rb, None)?),
        "fastq" => Box::new(parsers::fastq::FastqReader::new(rb, None)?),
        "flow" => Box::new(parsers::flow::FcsReader::new(rb, None)?),
        "inficon" => Box::new(parsers::inficon::InficonReader::new(rb, None)?),
        #[cfg(feature = "std")]
        "masshunter_dad" => Box::new(parsers::agilent::masshunter::MasshunterDadReader::new(
            rb,
            Some(
                params
                    .remove("filename")
                    .ok_or_else(|| "No filename found".into())
                    .and_then(Value::into_string)?,
            ),
        )?),
        #[cfg(feature = "std")]
        "png" => Box::new(parsers::png::PngReader::new(rb, None)?),
        "sam" => Box::new(parsers::sam::SamReader::new(rb, None)?),
        "thermo_cf" => Box::new(parsers::thermo::thermo_iso::ThermoCfReader::new(rb, None)?),
        "thermo_dxf" => Box::new(parsers::thermo::thermo_iso::ThermoDxfReader::new(rb, None)?),
        "thermo_raw" => Box::new(parsers::thermo::thermo_raw::ThermoRawReader::new(rb, None)?),
        "tsv" => Box::new(parsers::tsv::TsvReader::new(
            rb,
            Some(parsers::tsv::TsvParams::default().delim(b'\t')),
        )?),
        x => return Err(format!("No parser available for the parser {}", x).into()),
    };
    drop(params.remove("filename"));
    if !params.is_empty() {
        let keys: Vec<&str> = params.keys().map(AsRef::as_ref).collect();
        return Err(format!("Unused params remain: {}", keys.join(",")).into());
    }
    Ok((reader, parser_name))
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
    ///
    /// # Errors
    /// If the record can't be read, an error is returned.
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
    ($(#[$attr:meta])* $reader: ident, $record:ty, $record_lt:ty, $state:ty, $new_params:ty) => {
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
            pub fn new<B>(data: B, params: Option<$new_params>) -> Result<Self, EtError> where
                B: ::core::convert::TryInto<$crate::buffer::ReadBuffer<'r>>,
                EtError: From<<B as ::core::convert::TryInto<$crate::buffer::ReadBuffer<'r>>>::Error>,
            {
                let (rb, state) = $crate::readers::init_state(data, params)?;
                Ok($reader { rb, state })
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
                Ok(self.next()?.map(|r| r.into()))
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

/// Set up a state and a `ReadBuffer` for parsing.
#[doc(hidden)]
#[inline]
pub fn init_state<'r, S, B, P>(data: B, params: Option<P>) -> Result<(ReadBuffer<'r>, S), EtError>
where
    B: TryInto<ReadBuffer<'r>>,
    EtError: From<<B as TryInto<ReadBuffer<'r>>>::Error>,
    S: for<'s> FromSlice<'s, 's, State = P>,
    P: Default,
{
    let mut buffer = data.try_into()?;
    if let Some(state) = buffer.next::<S>(&mut params.unwrap_or_default())? {
        Ok((buffer, state))
    } else {
        Err(format!(
            "Could not initialize state {}",
            ::core::any::type_name::<S>()
        )
        .into())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    #[cfg(all(feature = "compression", feature = "std"))]
    fn test_bad_fuzzes() -> Result<(), EtError> {
        let data: &[u8] = &[
            40, 181, 47, 253, 0, 106, 99, 1, 14, 64, 40, 181, 47, 253, 0, 106, 99, 1, 14, 64, 40,
            181, 47, 253, 0, 106, 99, 1, 14, 64, 40, 181, 47, 253, 0, 106, 99, 1, 14, 64, 40, 181,
            47, 253, 0, 106, 99, 1, 14, 64, 40, 181, 47, 253, 0, 106, 99, 1, 14, 64, 40, 181, 47,
            253, 0, 106, 99, 1, 14, 64, 40, 181, 47, 253, 0, 106, 99, 1, 14, 64, 40, 181, 47, 253,
            0, 106, 99, 1, 14, 64, 40, 181, 47, 253, 0, 106, 99, 1, 14, 64, 40, 181, 47, 253, 0,
            106, 99, 1, 14, 64, 40, 181, 47, 253, 0, 106, 99, 1, 14, 64, 40, 181, 47, 253, 0, 106,
            99, 1, 14, 64, 40, 253, 47, 253, 0, 106, 1, 14, 19,
        ];

        let (mut reader, _) = get_reader(data, None, None)?;
        assert!(reader.next_record().is_err());
        Ok(())
    }
}
