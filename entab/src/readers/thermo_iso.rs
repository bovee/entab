use alloc::borrow::{Cow, ToOwned};
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::char::{decode_utf16, REPLACEMENT_CHARACTER};
use core::marker::Copy;

use crate::parsers::{extract, extract_opt, Endian, FromSlice, SeekPattern, Skip};
use crate::record::StateMetadata;
use crate::EtError;
use crate::{impl_reader, impl_record};

/// A string serialized out by the MFC framework.
#[derive(Debug, Default)]
pub struct MfcString<'r>(Cow<'r, str>);

impl<'r> FromSlice<'r> for MfcString<'r> {
    type State = ();

    fn parse(
        rb: &[u8],
        _eof: bool,
        consumed: &mut usize,
        _state: &mut Self::State,
    ) -> Result<bool, EtError> {
        if rb.is_empty() {
            return Err(EtError::from("No MfcString").incomplete());
        }
        if rb[0] == 0xFF && rb.len() < 4 {
            return Err(EtError::from("Expected longer MfcString").incomplete());
        }
        let end = if rb[0] != 0xFF {
            1 + usize::from(rb[0])
        } else if rb[1..3] == [0xFF, 0xFF] {
            4 + usize::from(rb[3])
        } else if rb[1..3] == [0xFE, 0xFF] {
            4 + 2 * usize::from(rb[3])
        } else {
            return Err("Unknown string header".into());
        };
        if rb.len() < end {
            return Err(EtError::from("MfcString ended early").incomplete());
        }
        *consumed += end;

        Ok(true)
    }

    fn get(&mut self, rb: &'r [u8], _state: &Self::State) -> Result<(), EtError> {
        let (start, utf16) = if rb[0] != 0xFF {
            (1, false)
        } else if rb[1..3] == [0xFF, 0xFF] {
            (4, false)
        } else if rb[1..3] == [0xFE, 0xFF] {
            (4, true)
        } else {
            return Err("Unknown string header".into());
        };

        let string = if utf16 {
            let iter = (start..rb.len())
                .step_by(2)
                .map(|i| u16::from_le_bytes([rb[i], rb[i + 1]]));
            decode_utf16(iter)
                .map(|r| r.unwrap_or(REPLACEMENT_CHARACTER))
                .collect::<String>()
                .into()
        } else {
            alloc::str::from_utf8(&rb[start..])?.into()
        };
        self.0 = string;
        Ok(())
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

/// The current state of the ThermoDxfReader
#[derive(Debug)]
pub struct ThermoDxfState {
    first: bool,
    n_scans_left: usize,
    cur_mz_idx: usize,
    mzs: Vec<f64>,
    cur_time: f64,
    cur_mz: f64,
    cur_intensity: f64,
}

impl Default for ThermoDxfState {
    fn default() -> Self {
        ThermoDxfState {
            first: true,
            n_scans_left: 0,
            cur_mz_idx: 0,
            mzs: Vec::new(),
            cur_time: 0.,
            cur_mz: 0.,
            cur_intensity: 0.,
        }
    }
}

impl<'r> StateMetadata<'r> for ThermoDxfState {}

impl<'r> FromSlice<'r> for ThermoDxfState {
    type State = ();
}

/// A single data point from a Thermo DXF file
#[derive(Clone, Copy, Debug, Default)]
pub struct ThermoDxfRecord {
    /// The time the reading was taken at
    pub time: f64,
    /// The mz value of the reading
    pub mz: f64,
    /// The intensity recorded
    pub intensity: f64,
}

impl_record!(ThermoDxfRecord: time, mz, intensity);

impl<'r> FromSlice<'r> for ThermoDxfRecord {
    type State = &'r mut ThermoDxfState;

    fn parse(
        rb: &[u8],
        eof: bool,
        consumed: &mut usize,
        state: &mut Self::State,
    ) -> Result<bool, EtError> {
        let con = &mut 0;
        if state.n_scans_left == 0 {
            // it appears the last u32 before the `FFFF04`... CRawData header
            // is the number of sections in the data, but
            if state.first {
                if extract_opt::<SeekPattern>(rb, eof, con, b"CRawData")?.is_none() {
                    return Err("Could not find CRawData section".into());
                }
                state.first = false;
                // str plus a u32 (value 3) and a `2F00`
                let _ = extract::<&[u8]>(rb, con, 14)?;
            } else {
                // `8282` is the replacement for CRawData, but we pad it out a
                // little in our search to help with specificity
                if extract_opt::<SeekPattern>(
                    rb,
                    eof,
                    con,
                    b"\x00\x00\x00\x00\x00\x00\x00\x00\x82\x82\x03\x00\x00\x00\x2F\x00\xFF\xFE\xFF",
                )?
                .is_none()
                {
                    return Ok(false);
                }
                // only consume up the to the `FFFEFF` part b/c that's part of the
                // gas name CString
                let _ = extract::<&[u8]>(rb, con, 16)?;
            }

            let gas_name = extract::<MfcString>(rb, con, ())?.0;
            if gas_name == "" {
                return Ok(false);
            }
            // the gas name
            state.mzs = mzs_from_gas(&gas_name)?;

            // `FFFEFF00` and then three u32s (values 0, 1, 1)
            let _ = extract::<Skip>(rb, con, 16)?;

            if extract::<u8>(rb, con, Endian::Little)? == 0xFF {
                // CEvalGasData header and the u32 (value 1)
                let _ = extract::<Skip>(rb, con, 20)?;
            } else {
                // replacement sentinel (`8482`) and the u32 (value 1)
                let _ = extract::<Skip>(rb, con, 6)?;
            }

            let bytes_data = extract::<u32>(rb, con, Endian::Little)? as usize;
            state.n_scans_left = bytes_data / (4 + 8 * state.mzs.len());
            state.cur_mz_idx = 0;
        }
        state.n_scans_left -= 1;
        if state.cur_mz_idx == 0 {
            state.cur_time = f64::from(extract::<f32>(rb, con, Endian::Little)?);
        }

        state.cur_mz = state.mzs[state.cur_mz_idx];
        state.cur_intensity = extract::<f64>(rb, con, Endian::Little)?;
        state.cur_mz_idx = (state.cur_mz_idx + 1) % state.mzs.len();
        *consumed += *con;
        Ok(true)
    }

    fn get(&mut self, _rb: &'r [u8], state: &Self::State) -> Result<(), EtError> {
        self.time = state.cur_time / 60.;
        self.mz = state.cur_mz;
        self.intensity = state.cur_intensity;
        Ok(())
    }
}

impl_reader!(ThermoDxfReader, ThermoDxfRecord, ThermoDxfState, ());

/// The current state of the ThermoCfReader
#[derive(Debug, Default)]
pub struct ThermoCfState {
    n_scans_left: usize,
    cur_mz_idx: usize,
    mzs: Vec<f64>,
    cur_time: f64,
    cur_mz: f64,
    cur_intensity: f64,
}

impl<'r> StateMetadata<'r> for ThermoCfState {}

impl<'r> FromSlice<'r> for ThermoCfState {
    type State = ();
}

/// A single data point from a Thermo CF file
#[derive(Clone, Copy, Debug, Default)]
pub struct ThermoCfRecord {
    /// The time the reading was taken at
    pub time: f64,
    /// The mz value of the reading
    pub mz: f64,
    /// The intensity recorded
    pub intensity: f64,
}

impl_record!(ThermoCfRecord: time, mz, intensity);

impl<'r> FromSlice<'r> for ThermoCfRecord {
    type State = &'r mut ThermoCfState;

    fn parse(
        rb: &[u8],
        eof: bool,
        consumed: &mut usize,
        state: &mut Self::State,
    ) -> Result<bool, EtError> {
        let con = &mut 0;
        if state.n_scans_left == 0 {
            if extract_opt::<SeekPattern>(
                rb,
                eof,
                con,
                b"\xFF\xFE\xFF\x00\xFF\xFE\xFF\x08R\x00a\x00w\x00 \x00D\x00a\x00t\x00a\x00",
            )?
            .is_none()
            {
                return Ok(false);
            }
            // pattern and then 3 u32's (values 0, 2, 2)
            let _ = extract::<&[u8]>(rb, con, 36)?;
            // read the title and an additional `030000002C00`
            if extract::<u8>(rb, con, Endian::Little)? == 0xFF {
                // CRawDataScanStorage title
                let _ = extract::<&[u8]>(rb, con, 34)?;
            } else {
                // the title was elided (there's a 3E86 sentinel?)
                let _ = extract::<&[u8]>(rb, con, 10)?;
            }
            // Now there's a CString with the type of the gas
            // remove "Trace Data" from the front of the string
            let gas_type = extract::<MfcString>(rb, con, ())?.0[11..].to_owned();
            state.mzs = mzs_from_gas(&gas_type)?;

            // then 4 u32's (0, 2, 0, 4) and a FEF0 block
            let _ = extract::<&[u8]>(rb, con, 20)?;
            state.n_scans_left = extract::<u32>(rb, con, Endian::Little)? as usize;
            // sanity check our guess for the masses
            let n_mzs = extract::<u32>(rb, con, Endian::Little)? as usize;
            if n_mzs != state.mzs.len() {
                return Err(format!("Gas type {} has bad information", gas_type).into());
            }

            // then a CBinary header (or replacement sentinel) followed by a u32
            // (value 2), a FEF0 block, another u32 (value 2), and then the number
            // of bytes of data that follow (value = n_scans * (4 + 8 * n_mzs))
            if extract::<u8>(rb, con, Endian::Little)? == 0xFF {
                // CBinary title
                let _ = extract::<&[u8]>(rb, con, 28)?;
            } else {
                // the title was elided (there's a 4086 sentinel?)
                let _ = extract::<&[u8]>(rb, con, 18)?;
            }
        }
        state.n_scans_left -= 1;
        if state.cur_mz_idx == 0 {
            state.cur_time = f64::from(extract::<f32>(rb, con, Endian::Little)?);
        }

        state.cur_mz = state.mzs[state.cur_mz_idx];
        state.cur_intensity = extract::<f64>(rb, con, Endian::Little)?;
        state.cur_mz_idx = (state.cur_mz_idx + 1) % state.mzs.len();
        *consumed += *con;
        Ok(true)
    }

    fn get(&mut self, _rb: &'r [u8], state: &Self::State) -> Result<(), EtError> {
        self.time = state.cur_time / 60.;
        self.mz = state.cur_mz;
        self.intensity = state.cur_intensity;
        Ok(())
    }
}

impl_reader!(ThermoCfReader, ThermoCfRecord, ThermoCfState, ());

#[cfg(test)]
mod tests {
    use super::*;
    use crate::readers::RecordReader;

    #[test]
    fn test_thermo_dxf_reader() -> Result<(), EtError> {
        let rb: &[u8] = include_bytes!("../../tests/data/b3_alkanes.dxf");
        let mut reader = ThermoDxfReader::new(rb, ())?;
        let _ = reader.metadata();
        if let Some(ThermoDxfRecord {
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
        while reader.next()?.is_some() {}
        Ok(())
    }

    #[test]
    fn test_thermo_dxf_bad_fuzzes() -> Result<(), EtError> {
        let test_data = [
            255, 255, 5, 0, 0, 0, 32, 0, 0, 67, 82, 97, 119, 68, 97, 116, 97, 0, 10, 255, 255, 0,
            255, 255, 255, 255, 255, 0, 0, 0, 0,
        ];
        let mut reader = ThermoDxfReader::new(&test_data[..], ())?;
        assert!(reader.next().is_err());
        Ok(())
    }

    #[test]
    fn test_thermo_cf_reader() -> Result<(), EtError> {
        let rb: &[u8] = include_bytes!("../../tests/data/test-0000.cf");
        let mut reader = ThermoCfReader::new(rb, ())?;
        let _ = reader.metadata();
        if let Some(ThermoCfRecord {
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
        while reader.next()?.is_some() {}
        Ok(())
    }
}
