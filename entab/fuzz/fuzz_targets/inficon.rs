#![no_main]
use libfuzzer_sys::fuzz_target;
extern crate entab;

use entab::EtError;
use entab::readers::inficon::InficonReader;

fuzz_target!(|data: &[u8]| {
    let _ = parse_data(data);
});

fn parse_data(data: &[u8]) -> Result<(), EtError> {
    let mut rec_reader = InficonReader::new(data, (Vec::new(), 0usize))?;
    while let Some(_) = rec_reader.next()? {
    }
    Ok(())
}
