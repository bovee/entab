#![no_main]
use libfuzzer_sys::fuzz_target;
extern crate entab;

use entab::EtError;
use entab::filetype::FileType;
use entab::readers::get_reader;

fuzz_target!(|data: &[u8]| {
    let _ = generate_reader(data);
});

fn generate_reader(data: &[u8]) -> Result<(), EtError> {
    let filetype = FileType::from_magic(&data);
    let mut rec_reader = get_reader(filetype, data)?;
    while let Some(_) = rec_reader.next_record()? {
    }
    Ok(())
}
