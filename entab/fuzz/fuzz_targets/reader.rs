#![no_main]
use libfuzzer_sys::fuzz_target;
extern crate entab;

use entab::buffer::ReadBuffer;
use entab::filetype::FileType;
use entab::EtError;
use entab::readers::get_builder;

fuzz_target!(|data: &[u8]| {
    let _ = generate_reader(data);
});

fn generate_reader(data: &[u8]) -> Result<(), EtError> {
    let filetype = FileType::from_magic(&data);
    let rb = ReadBuffer::from_slice(&data);
    if let Some(builder) = get_builder(filetype.to_parser_name()) {
        let mut rec_reader = builder.to_reader(rb)?;
        while let Some(_) = rec_reader.next()? {
        }
    };
    Ok(())
}
