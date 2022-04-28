#![no_main]
use libfuzzer_sys::fuzz_target;
extern crate entab;

use entab::EtError;
use entab::parsers::tsv::TsvReader;

fuzz_target!(|data: &[u8]| {
    let _ = generate_reader(data);
});

fn generate_reader(data: &[u8]) -> Result<(), EtError> {
    let mut reader = TsvReader::new(data, None)?;
    while let Some(_) = reader.next()? {
    }
    Ok(())
}
