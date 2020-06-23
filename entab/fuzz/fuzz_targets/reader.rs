#![no_main]
use libfuzzer_sys::fuzz_target;
extern crate entab;

use entab::buffer::ReadBuffer;
use entab::filetype::FileType;
use entab::{all_types, EtError};
use entab::record::{ReaderBuilder, BindT};

fuzz_target!(|data: &[u8]| {
    let _ = generate_reader(data);
});

fn generate_reader(data: &[u8]) -> Result<(), EtError> {
    let filetype = FileType::from_magic(&data);
    let rb = ReadBuffer::from_slice(&data);
    all_types!(match filetype.to_parser_name() => test_reader::<>(rb))?;
    Ok(())
}

fn test_reader<R>(buffer: ReadBuffer) -> Result<(), EtError>
where
    R: ReaderBuilder,
    R::Item: for<'a> BindT<'a>,
{
    let mut rec_reader = R::default().to_reader(buffer)?;
    while let Some(_) = rec_reader.next()? {
    }
    Ok(())
}
