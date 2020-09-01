use alloc::borrow::{Cow, ToOwned};
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::char::{decode_utf16, REPLACEMENT_CHARACTER};

use crate::buffer::ReadBuffer;
use crate::impl_reader;
use crate::parsers::{Endian, FromBuffer};
use crate::record::Record;
use crate::EtError;

pub struct CString<'r>(Cow<'r, str>);

impl<'r> FromBuffer<'r> for CString<'r> {
    type State = ();

    fn get(rb: &'r mut ReadBuffer, _state: Self::State) -> Result<Self, EtError> {
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
        let string = if utf16 {
            let iter = (0..end - start)
                .step_by(2)
                .map(|i| u16::from_le_bytes([data[i], data[i + 1]]));
            decode_utf16(iter)
                .map(|r| r.unwrap_or(REPLACEMENT_CHARACTER))
                .collect::<String>()
                .into()
        } else {
            alloc::str::from_utf8(data)?.into()
        };
        Ok(CString(string))
    }
}

fn mzs_from_gas(gas: &str) -> Result<Vec<f64>, EtError> {
    Ok(match gas {
        "CO2" => vec![44., 45., 46.],
        "CO" => vec![28., 29., 30.],
        "H2" => vec![2., 3.],
        "N2" => vec![28., 29., 30.],
        "SO2" => vec![64., 66.],
        "SO2,SO-SO2 Ext,SO" => vec![48., 49., 50., 64., 65., 66.],
        i => return Err(format!("Gas type {} not supported yet", i).into()),
    })
}

pub struct ThermoDxfState {
    first: bool,
    n_scans_left: usize,
    cur_mz_idx: usize,
    mzs: Vec<f64>,
    cur_time: f64,
}

impl<'r> FromBuffer<'r> for ThermoDxfState {
    type State = ();

    fn get(_rb: &'r mut ReadBuffer, _state: Self::State) -> Result<Self, EtError> {
        Ok(ThermoDxfState {
            first: true,
            n_scans_left: 0,
            cur_mz_idx: 0,
            mzs: vec![],
            cur_time: 0.,
        })
    }
}

pub struct ThermoDxfRecord {
    time: f64,
    mz: f64,
    intensity: f64,
}

impl<'r> From<ThermoDxfRecord> for Record<'r> {
    fn from(record: ThermoDxfRecord) -> Self {
        Record::Mz {
            time: record.time,
            mz: record.mz,
            intensity: record.intensity,
        }
    }
}

impl<'r> FromBuffer<'r> for Option<ThermoDxfRecord> {
    type State = &'r mut ThermoDxfState;

    fn get(rb: &'r mut ReadBuffer, state: Self::State) -> Result<Self, EtError> {
        if state.n_scans_left == 0 {
            // it appears the last u32 before the `FFFF04`... CRawData header
            // is the number of sections in the data, but
            if state.first {
                if !rb.seek_pattern(b"CRawData")? {
                    return Err("Could not find data".into());
                }
                state.first = false;
                // str plus a u32 (value 3) and a `2F00`
                rb.reserve(14)?;
                rb.partial_consume(14);
            } else {
                // `8282` is the replacement for CRawData, but we pad it out a
                // little in our search to help with specificity
                if !rb.seek_pattern(
                    b"\x00\x00\x00\x00\x00\x00\x00\x00\x82\x82\x03\x00\x00\x00\x2F\x00\xFF\xFE\xFF",
                )? {
                    return Ok(None);
                }
                // only consume up the to the `FFFEFF` part b/c that's part of the
                // gas name CString
                rb.partial_consume(16);
            }

            let gas_name = rb.extract::<CString>(())?.0;
            if gas_name == "" {
                return Ok(None);
            }
            // the gas name
            state.mzs = mzs_from_gas(&gas_name)?;

            // `FFFEFF00` and then three u32s (values 0, 1, 1)
            rb.reserve(16)?;
            rb.partial_consume(16);

            if rb.extract::<u8>(Endian::Little)? == 0xFF {
                // CEvalGasData header and the u32 (value 1)
                rb.reserve(20)?;
                rb.partial_consume(20);
            } else {
                // replacement sentinel (`8482`) and the u32 (value 1)
                rb.reserve(6)?;
                rb.partial_consume(6);
            }

            let bytes_data = rb.extract::<u32>(Endian::Little)? as usize;
            state.n_scans_left = bytes_data / (4 + 8 * state.mzs.len());
            state.cur_mz_idx = 0;
        }
        state.n_scans_left -= 1;
        if state.cur_mz_idx == 0 {
            rb.reserve(12)?;
            state.cur_time = f64::from(rb.extract::<f32>(Endian::Little)?);
        } else {
            rb.reserve(8)?;
        }

        let intensity = rb.extract::<f64>(Endian::Little)?;
        let mz = state.mzs[state.cur_mz_idx];
        state.cur_mz_idx = (state.cur_mz_idx + 1) % state.mzs.len();

        Ok(Some(ThermoDxfRecord {
            time: state.cur_time / 60.,
            mz,
            intensity,
        }))
    }
}

impl_reader!(ThermoDxfReader, ThermoDxfRecord, ThermoDxfState, ());

pub struct ThermoCfState {
    n_scans_left: usize,
    cur_mz_idx: usize,
    mzs: Vec<f64>,
    cur_time: f64,
}

impl<'r> FromBuffer<'r> for ThermoCfState {
    type State = ();

    fn get(_rb: &'r mut ReadBuffer, _state: Self::State) -> Result<Self, EtError> {
        Ok(ThermoCfState {
            n_scans_left: 0,
            cur_mz_idx: 0,
            mzs: vec![],
            cur_time: 0.,
        })
    }
}

pub struct ThermoCfRecord {
    time: f64,
    mz: f64,
    intensity: f64,
}

impl<'r> From<ThermoCfRecord> for Record<'r> {
    fn from(record: ThermoCfRecord) -> Self {
        Record::Mz {
            time: record.time,
            mz: record.mz,
            intensity: record.intensity,
        }
    }
}

impl<'r> FromBuffer<'r> for Option<ThermoCfRecord> {
    type State = &'r mut ThermoCfState;

    fn get(rb: &'r mut ReadBuffer, state: Self::State) -> Result<Self, EtError> {
        if state.n_scans_left == 0 {
            if !rb.seek_pattern(
                b"\xFF\xFE\xFF\x00\xFF\xFE\xFF\x08R\x00a\x00w\x00 \x00D\x00a\x00t\x00a\x00",
            )? {
                return Ok(None);
            }
            // pattern and then 3 u32's (values 0, 2, 2)
            rb.reserve(36)?;
            rb.partial_consume(36);
            // read the title and an additional `030000002C00`
            if rb.extract::<u8>(Endian::Little)? == 0xFF {
                // CRawDataScanStorage title
                rb.reserve(34)?;
                rb.partial_consume(34);
            } else {
                // the title was elided (there's a 3E86 sentinel?)
                rb.reserve(10)?;
                rb.partial_consume(10);
            }
            // Now there's a CString with the type of the gas
            // remove "Trace Data" from the front of the string
            let gas_type = rb.extract::<CString>(())?.0[11..].to_owned();
            state.mzs = mzs_from_gas(&gas_type)?;

            // then 4 u32's (0, 2, 0, 4) and a FEF0 block
            rb.reserve(20)?;
            rb.partial_consume(20);
            state.n_scans_left = rb.extract::<u32>(Endian::Little)? as usize;
            // sanity check our guess for the masses
            let n_mzs = rb.extract::<u32>(Endian::Little)? as usize;
            if n_mzs != state.mzs.len() {
                return Err(format!("Gas type {} has bad information", gas_type).into());
            }

            // then a CBinary header (or replacement sentinel) followed by a u32
            // (value 2), a FEF0 block, another u32 (value 2), and then the number
            // of bytes of data that follow (value = n_scans * (4 + 8 * n_mzs))
            if rb.extract::<u8>(Endian::Little)? == 0xFF {
                // CBinary title
                rb.reserve(28)?;
                rb.partial_consume(28);
            } else {
                // the title was elided (there's a 4086 sentinel?)
                rb.reserve(18)?;
                rb.partial_consume(18);
            }
        }
        state.n_scans_left -= 1;
        if state.cur_mz_idx == 0 {
            rb.reserve(12)?;
            state.cur_time = f64::from(rb.extract::<f32>(Endian::Little)?);
        } else {
            rb.reserve(8)?;
        }

        let intensity = rb.extract::<f64>(Endian::Little)?;
        let mz = state.mzs[state.cur_mz_idx];
        state.cur_mz_idx = (state.cur_mz_idx + 1) % state.mzs.len();

        Ok(Some(ThermoCfRecord {
            time: state.cur_time / 60.,
            mz,
            intensity,
        }))
    }
}

impl_reader!(ThermoCfReader, ThermoCfRecord, ThermoCfState, ());

#[cfg(test)]
mod tests {
    #[cfg(feature = "std")]
    use super::*;
    #[cfg(feature = "std")]
    use crate::buffer::ReadBuffer;
    use crate::readers::RecordReader;

    #[cfg(feature = "std")]
    #[test]
    fn test_thermo_dxf_reader() -> Result<(), EtError> {
        use std::fs::File;

        let f = File::open("tests/data/b3_alkanes.dxf")?;
        let rb = ReadBuffer::new(Box::new(&f))?;
        let mut reader = ThermoDxfReader::new(rb, ())?;
        if let Some(Record::Mz {
            time,
            mz,
            intensity,
        }) = reader.next()?
        {
            assert!((time - 0.03135).abs() < 0.000001);
            assert!((mz - 44.).abs() < 0.000001);
            assert!((intensity - 2.015212).abs() < 0.000001);
        } else {
            panic!("Thermo DXF reader returned bad record");
        }
        while let Some(_) = reader.next()? {}
        Ok(())
    }

    #[cfg(feature = "std")]
    #[test]
    fn test_thermo_cf_reader() -> Result<(), EtError> {
        use std::fs::File;

        let f = File::open("tests/data/test-0000.cf")?;
        let rb = ReadBuffer::new(Box::new(&f))?;
        let mut reader = ThermoCfReader::new(rb, ())?;
        if let Some(Record::Mz {
            time,
            mz,
            intensity,
        }) = reader.next()?
        {
            assert!((time - 0.003483).abs() < 0.000001);
            assert!((mz - 44.).abs() < 0.000001);
            assert!((intensity - 4093.056638).abs() < 0.000001);
        } else {
            panic!("Thermo CF reader returned bad record");
        }
        while let Some(_) = reader.next()? {}
        Ok(())
    }
}