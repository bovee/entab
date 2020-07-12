use alloc::borrow::Cow;
use alloc::boxed::Box;
use alloc::format;
use alloc::vec;
use alloc::vec::Vec;
use core::char::{decode_utf16, REPLACEMENT_CHARACTER};

use byteorder::{ByteOrder, LittleEndian};
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

fn parse_cstring<'r>(rb: &'r mut ReadBuffer) -> Result<Cow<'r, str>, EtError> {
    if rb.is_empty() {
        rb.reserve(1)?;
    }
    if rb[0] == 0xFF && rb.len() < 4 {
        rb.reserve(4)?;
    }
    let (start, end, utf16) = if rb[0] != 0xFF {
        (1, 1 + usize::from(rb[0]), false)
    } else if rb[1..3] == [0xFF, 0xFF] {
        (4, 4 + usize::from(rb[3]), false)
    } else if rb[1..3] == [0xFE, 0xFF] {
        (4, 4 + 2 * usize::from(rb[3]), true)
    } else {
        return Err("Unknown string header".into());
    };
    if rb.len() < end {
        rb.reserve(end)?;
    }

    let data = &rb.partial_consume(end)[start..];
    Ok(if utf16 {
        let iter = (0..end - start)
            .step_by(2)
            .map(|i| u16::from_le_bytes([data[i], data[i + 1]]));
        decode_utf16(iter)
            .map(|r| r.unwrap_or(REPLACEMENT_CHARACTER))
            .collect::<String>()
            .into()
    } else {
        alloc::str::from_utf8(data)?.into()
    })
}

impl ReaderBuilder for ThermoMsReaderBuilder {
    fn to_reader<'r>(&self, mut rb: ReadBuffer<'r>) -> Result<Box<dyn RecordReader + 'r>, EtError> {
        if !rb.seek_pattern(b"CRawData")? {
            return Err("Could not find data".into());
        }
        rb.partial_consume(8 + 6);
        let mzs = match parse_cstring(&mut rb)?.as_ref() {
            "CO2" => vec![44., 45., 46.],
            "CO" => vec![28., 29., 30.],
            "SO2,SO-SO2 Ext,SO" => vec![48., 49., 50., 64., 65., 66.],
            i => return Err(format!("Gas type {} not supported yet", i).into()),
        };

        if !rb.seek_pattern(b"CEvalGCData")? {
            return Err("Could not find data".into());
        }
        rb.partial_consume(11 + 4);
        let n_scans_left =
            LittleEndian::read_u32(rb.partial_consume(4)) as usize / (4 + 8 * mzs.len());

        Ok(Box::new(ThermoMsReader {
            rb,
            n_scans_left,
            cur_mz_idx: 0,
            mzs,
            cur_time: 0.,
        }))
    }
}

pub struct ThermoMsReader<'r> {
    rb: ReadBuffer<'r>,
    n_scans_left: usize,
    cur_mz_idx: usize,
    mzs: Vec<f64>,
    cur_time: f64,
}

impl<'r> RecordReader for ThermoMsReader<'r> {
    fn next(&mut self) -> Result<Option<Record>, EtError> {
        if self.n_scans_left == 0 {
            return Ok(None);
        }
        self.n_scans_left -= 1;
        if self.cur_mz_idx == 0 {
            self.rb.reserve(12)?;
            self.cur_time = f64::from(LittleEndian::read_f32(self.rb.partial_consume(4)));
        } else {
            self.rb.reserve(8)?;
        }

        let intensity = LittleEndian::read_f64(self.rb.consume(8));
        let mz = self.mzs[self.cur_mz_idx];
        self.cur_mz_idx = (self.cur_mz_idx + 1) % self.mzs.len();

        Ok(Some(Record::MzFloat {
            time: self.cur_time / 60.,
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
        if let Some(Record::MzFloat {
            time,
            mz,
            intensity,
        }) = reader.next()?
        {
            assert!((time - 0.03135).abs() < 0.000001);
            assert!((mz - 44.).abs() < 0.000001);
            assert!((intensity - 2.015212).abs() < 0.000001);
        } else {
            panic!("Thermo reader returned bad record");
        }
        while let Some(_) = reader.next()? {}
        Ok(())
    }
}
