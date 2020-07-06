use alloc::borrow::Cow;
use alloc::boxed::Box;
use alloc::format;
use alloc::vec;
use alloc::vec::Vec;
use core::char::{decode_utf16, REPLACEMENT_CHARACTER};

use byteorder::{BigEndian, ByteOrder, LittleEndian};
use serde::Serialize;

use crate::buffer::ReadBuffer;
use crate::record::{ReaderBuilder, Record, RecordReader};
use crate::EtError;

#[derive(Debug, Serialize)]
pub struct FloatMzRecord {
    time: f64,
    mz: f64,
    intensity: f64,
}

pub struct ThermoMsReaderBuilder;

impl Default for ThermoMsReaderBuilder {
    fn default() -> Self {
        ThermoMsReaderBuilder
    }
}

// TODO: need seek/find function?

fn parse_cstring(data: &[u8]) -> Result<Cow<str>, EtError> {
    let (pos, utf16) = if data[0] != 0xFF {
        (1..usize::from(data[0]), false)
    } else if data[1..2] == [0xFF, 0xFF] {
        (7..LittleEndian::read_u64(&data[3..7]) as usize, false)
    } else if data[1..2] == [0xFE, 0xFF] {
        (7..LittleEndian::read_u64(&data[3..7]) as usize, true)
    } else {
        (3..usize::from(LittleEndian::read_u16(&data[1..2])), false)
    };

    Ok(if utf16 {
        let iter = pos.map(|i| u16::from_le_bytes([data[2 * i], data[2 * i + 1]]));
        decode_utf16(iter)
            .map(|r| r.unwrap_or(REPLACEMENT_CHARACTER))
            .collect::<String>()
            .into()
    } else {
        alloc::str::from_utf8(&data[pos])?.into()
    })
}

impl ReaderBuilder for ThermoMsReaderBuilder {
    fn to_reader<'r>(&self, mut rb: ReadBuffer<'r>) -> Result<Box<dyn RecordReader + 'r>, EtError> {
        // TODO: find the start of the data?

        Ok(Box::new(ThermoMsReader { rb }))
    }
}

pub struct ThermoMsReader<'r> {
    rb: ReadBuffer<'r>,
    n_scans_left: usize,
    n_mzs_left: usize,
    cur_time: f64,
}

impl<'r> RecordReader for ThermoMsReader<'r> {
    fn next(&mut self) -> Result<Option<Record>, EtError> {
        if self.n_scans_left == 0 {
            return Ok(None);
        }
        let mz = 0.;
        let intensity = 0.;

        Ok(Some(Record::MzFloat {
            time: self.cur_time,
            mz,
            intensity,
        }))
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "std")]
    use super::*;
    #[cfg(feature = "std")]
    use crate::buffer::ReadBuffer;

    #[cfg(feature = "std")]
    #[test]
    fn test_thermo_reader() -> Result<(), EtError> {
        use std::fs::File;

        let f = File::open("tests/data/b3_alkanes.dxf")?;
        let rb = ReadBuffer::new(Box::new(&f))?;
        let builder = ThermoMsReaderBuilder::default();
        let mut reader = builder.to_reader(rb)?;
        // while let Some(_) = reader.next()? {}
        Ok(())
    }
}
