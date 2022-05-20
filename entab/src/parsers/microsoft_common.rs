use core::convert::TryInto;

use chrono::{NaiveDateTime, TimeZone, Utc};

use crate::error::EtError;
use crate::parsers::{extract, Endian, FromSlice};


/// Convert a "Windows" timestamp into a regular `DateTime`.
///
/// Windows time is the number of "100 microsecond" chunks since January 1, 1601 so to convert to
/// unix time we first need to convert into nanoseconds and then subtract the number of nanoseconds
/// from then to Jan 1, 1970.
pub fn from_windows_time(time: u64) -> Result<NaiveDateTime, EtError> {
    let unix_time = time.saturating_mul(100).saturating_sub(11_644_473_600_000_000_000);
    Ok(Utc.timestamp_nanos(unix_time.try_into()?).naive_local())
}

/// A chunk from a Microsoft "Compound File Binary" file (commonly used on Windows machines to
/// store different data).
///
/// See Microsoft documentation for more info:
/// https://docs.microsoft.com/en-us/openspecs/windows_protocols/ms-cfb/05060311-bfce-4b12-874d-71fd4ce63aea
#[derive(Debug, Default)]
struct MsCfbHeader { }

impl<'b: 's, 's> FromSlice<'b, 's> for MsCfbHeader {
    type State = ();

    fn parse(
        buffer: &[u8],
        _eof: bool,
        _consumed: &mut usize,
        _state: &mut Self::State,
    ) -> Result<bool, EtError> {
        const CFB_MAGIC: &[u8] = b"\xD0\xCF\x11\xE0\xA1\xB1\x1A\xE1";

        if buffer.len() < 512 {
            return Err(EtError::new("MS CFB headers are always 512 bytes long").incomplete());
        }
        if &buffer[..8] != CFB_MAGIC {
            return Err(EtError::new("CFB header has invalid magic"));
        }

        // minor_version = buffer[24..26]
        // major_version = buffer[26..28]
        // byte_order = buffer[28..30]
        let sector_size = match buffer[30..32] {
            [0x09, 0] => 512,
            [0x0C, 0] => 4096,
            _ => return Err("Invalid sector shift specified".into()),
        };
        // 32..44 -> ...
    
        let n_fat_sectors = u32::extract(&buffer[44..48], Endian::Little)?;
        // TODO: we could maybe come up with a way to not call the `parse` side of above, but with
        // good ergonomics? (the below is a little gross)
        // let mut n_fat_sectors: u32 = 0;
        // FromSlice::get(&mut n_fat_sectors, &buffer[44..48], &Endian::Little)?;

        let first_dir_loc = u32::extract(&buffer[48..52], Endian::Little)?;
        let first_minifat_loc = u32::extract(&buffer[60..64], Endian::Little)?;
        let n_minifat_sectors = u32::extract(&buffer[64..68], Endian::Little)?;
        let first_difat_loc = u32::extract(&buffer[68..72], Endian::Little)?;
        let n_difat_sectors = u32::extract(&buffer[72..76], Endian::Little)?;
        if n_difat_sectors > 0 {
            return Err("DIFAT sectors aren't supported yet".into());
        }
        // 76..512 -> DIFAT array of u32s

        Ok(false)
    }

    fn get(&mut self, _buffer: &'r [u8], _state: &Self::State) -> Result<(), EtError> {
        Ok(())
    }
}
