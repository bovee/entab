use alloc::collections::BTreeMap;
use alloc::str;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::marker::Copy;

use crate::parsers::agilent::metadata::ChemstationMetadata;
use crate::parsers::agilent::read_agilent_header;
use crate::parsers::{extract, Endian, FromSlice};
use crate::record::{StateMetadata, Value};
use crate::EtError;
use crate::{impl_reader, impl_record};

#[derive(Clone, Debug, Default)]
/// Internal state for the `ChemstationUvRecord` parser
pub struct ChemstationUvState {
    metadata: ChemstationMetadata,
    n_scans_left: usize,
    n_wvs_left: usize,
    cur_time: f64,
    cur_intensity: f64,
    cur_wv: f64,
    wv_step: f64,
}

impl StateMetadata for ChemstationUvState {
    fn metadata(&self) -> BTreeMap<String, Value> {
        (&self.metadata).into()
    }

    fn header(&self) -> Vec<&str> {
        vec!["time", "wavelength", "intensity"]
    }
}

impl<'b: 's, 's> FromSlice<'b, 's> for ChemstationUvState {
    type State = ();

    fn parse(
        rb: &[u8],
        _eof: bool,
        consumed: &mut usize,
        _state: &mut Self::State,
    ) -> Result<bool, EtError> {
        *consumed += read_agilent_header(rb, false)?;
        Ok(true)
    }

    fn get(&mut self, rb: &'b [u8], _state: &'s Self::State) -> Result<(), EtError> {
        let n_scans = u32::extract(&rb[278..], &Endian::Big)? as usize;

        self.metadata = ChemstationMetadata::from_header(rb)?;
        self.n_scans_left = n_scans;
        self.n_wvs_left = 0;
        self.cur_time = 0.;
        self.cur_wv = 0.;
        self.cur_intensity = 0.;
        self.wv_step = 0.;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default)]
/// A record from a Chemstation UV file
pub struct ChemstationUvRecord {
    /// The time recorded at
    pub time: f64,
    /// The wavelength recorded at
    pub wavelength: f64,
    /// The intensity record
    pub intensity: f64,
}

impl_record!(ChemstationUvRecord: time, wavelength, intensity);

impl<'b: 's, 's> FromSlice<'b, 's> for ChemstationUvRecord {
    type State = ChemstationUvState;

    fn parse(
        rb: &[u8],
        _eof: bool,
        consumed: &mut usize,
        state: &mut Self::State,
    ) -> Result<bool, EtError> {
        if state.n_scans_left == 0 {
            return Ok(false);
        }

        let con = &mut 0;
        // refill case
        let mut n_wvs_left = state.n_wvs_left;
        //
        if n_wvs_left == 0 {
            let _ = extract::<&[u8]>(rb, con, &mut 4)?; // 67, 624/224
            state.cur_time = f64::from(extract::<u32>(rb, con, &mut Endian::Little)?) / 60000.;
            let wv_start: u16 = extract(rb, con, &mut Endian::Little)?;
            let wv_end: u16 = extract(rb, con, &mut Endian::Little)?;
            if wv_start > wv_end {
                return Err("Wavelength range has invalid bounds".into());
            }
            let wv_step: u16 = extract(rb, con, &mut Endian::Little)?;
            if wv_step == 0 {
                return Err("Invalid wavelength step".into());
            }

            n_wvs_left = usize::from((wv_end - wv_start) / wv_step) + 1;
            state.wv_step = f64::from(wv_step) / 20.;
            state.cur_wv = f64::from(wv_start) / 20. - state.wv_step;
            state.cur_intensity = 0.;
            let _ = extract::<&[u8]>(rb, con, &mut 8)?; // 80/53, 4, 400, 0
        };

        let delta = extract::<i16>(rb, con, &mut Endian::Little)?;
        if delta == -32768 {
            state.cur_intensity = f64::from(extract::<i32>(rb, con, &mut Endian::Little)?);
        } else {
            state.cur_intensity += f64::from(delta);
        }

        if state.n_wvs_left == 1 {
            state.n_scans_left -= 1;
        }
        state.cur_wv += state.wv_step;
        state.n_wvs_left = n_wvs_left - 1;
        *consumed += *con;
        Ok(true)
    }

    fn get(&mut self, _rb: &'b [u8], state: &'s Self::State) -> Result<(), EtError> {
        self.time = state.cur_time;
        self.wavelength = state.cur_wv;
        self.intensity = state.cur_intensity * state.metadata.mult_correction;
        Ok(())
    }
}

impl_reader!(
    ChemstationUvReader,
    ChemstationUvRecord,
    ChemstationUvRecord,
    ChemstationUvState,
    ()
);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
/// The type of the records in the array.
pub enum ChemstationArrayRecordType {
    #[default]
    /// All of the values are stored as f32
    Float32Array,
    /// All of the values are stored as f64
    Float64Array,
}

#[derive(Clone, Debug, Default)]
/// Internal state for the `ChemstationArrayRecord` parser
pub struct ChemstationArrayState {
    metadata: ChemstationMetadata,
    record_type: ChemstationArrayRecordType,
    n_scans_left: usize,
    cur_time: f64,
    time_step: f64,
}

impl StateMetadata for ChemstationArrayState {
    fn metadata(&self) -> BTreeMap<String, Value> {
        (&self.metadata).into()
    }

    fn header(&self) -> Vec<&str> {
        vec!["time", "intensity"]
    }
}

impl<'b: 's, 's> FromSlice<'b, 's> for ChemstationArrayState {
    type State = ();

    fn parse(
        rb: &[u8],
        _eof: bool,
        consumed: &mut usize,
        _state: &mut Self::State,
    ) -> Result<bool, EtError> {
        *consumed += read_agilent_header(rb, false)?;
        Ok(true)
    }

    fn get(&mut self, rb: &'b [u8], _state: &'s Self::State) -> Result<(), EtError> {
        self.metadata = ChemstationMetadata::from_header(rb)?;

        let record_type = if &rb[348..352] == b"G\x00C\x00"
            || &rb[3090..3104] == b"M\x00u\x00s\x00t\x00a\x00n\x00g\x00"
        {
            ChemstationArrayRecordType::Float64Array
        } else {
            ChemstationArrayRecordType::Float32Array
        };

        let tstep_num = u16::extract(&rb[4122..], &Endian::Big)? as f64;
        let tstep_denom = u16::extract(&rb[4124..], &Endian::Big)? as f64;
        let tstep = (tstep_num / tstep_denom) / 60.;

        // The file from issue #42 has 12000 scans, but the field at 278 only says 197?
        // The other file I have is correct so maybe that's corrupt, but we're using
        // the time step to figure this out for now.
        // let n_scans = u32::extract(&rb[278..], &Endian::Big)? as usize;
        let n_scans = 1 + ((self.metadata.end_time - self.metadata.start_time) / tstep) as usize;

        self.n_scans_left = n_scans;
        self.record_type = record_type;
        self.cur_time = self.metadata.start_time;
        self.time_step = tstep;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default)]
/// A record from a Chemstation UV file
pub struct ChemstationArrayRecord {
    /// The time recorded at
    pub time: f64,
    /// The intensity recorded
    pub intensity: f64,
}

impl_record!(ChemstationArrayRecord: time, intensity);

impl<'b: 's, 's> FromSlice<'b, 's> for ChemstationArrayRecord {
    type State = ChemstationArrayState;

    fn parse(
        _rb: &[u8],
        _eof: bool,
        consumed: &mut usize,
        state: &mut Self::State,
    ) -> Result<bool, EtError> {
        if state.n_scans_left == 0 {
            return Ok(false);
        }
        *consumed += match state.record_type {
            ChemstationArrayRecordType::Float32Array => 4,
            ChemstationArrayRecordType::Float64Array => 8,
        };
        state.n_scans_left -= 1;
        state.cur_time += state.time_step;
        Ok(true)
    }

    fn get(&mut self, rb: &'b [u8], state: &'s Self::State) -> Result<(), EtError> {
        let con = &mut 0;
        let intensity = match state.record_type {
            ChemstationArrayRecordType::Float32Array => {
                extract::<f32>(rb, con, &mut Endian::Little)? as f64
            }
            ChemstationArrayRecordType::Float64Array => {
                extract::<f64>(rb, con, &mut Endian::Little)?
            }
        };

        self.time = state.cur_time;
        self.intensity = intensity * state.metadata.mult_correction;
        Ok(())
    }
}

impl_reader!(
    ChemstationArrayReader,
    ChemstationArrayRecord,
    ChemstationArrayRecord,
    ChemstationArrayState,
    ()
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::readers::RecordReader;

    #[test]
    fn test_chemstation_reader_uv() -> Result<(), EtError> {
        let data: &[u8] = include_bytes!("../../../tests/data/carotenoid_extract.d/dad1.uv");
        let mut reader = ChemstationUvReader::new(data, None)?;
        let _ = reader.metadata();
        assert_eq!(reader.headers(), ["time", "wavelength", "intensity"]);

        let ChemstationUvRecord {
            time,
            wavelength,
            intensity,
        } = reader.next()?.unwrap();
        assert!((time - 0.001333).abs() < 0.000001);
        assert!((wavelength - 200.).abs() < 0.000001);
        assert_eq!(intensity, -14.941692352294922);

        let ChemstationUvRecord {
            time,
            wavelength,
            intensity,
        } = reader.next()?.unwrap();
        assert!((time - 0.001333).abs() < 0.000001);
        assert!((wavelength - 202.).abs() < 0.000001);
        assert_eq!(intensity, -30.33161163330078);

        let mut n_mzs = 2;
        while reader.next()?.is_some() {
            n_mzs += 1;
        }
        assert_eq!(n_mzs, 6744 * 301);
        Ok(())
    }

    #[test]
    fn test_array_chemstation_reader() -> Result<(), EtError> {
        let data: &[u8] = include_bytes!("../../../tests/data/test_179_fid.ch");
        let mut reader = ChemstationArrayReader::new(data, None)?;
        let _ = reader.metadata();
        assert_eq!(reader.headers(), ["time", "intensity"]);

        let ChemstationArrayRecord { time, intensity } = reader.next()?.unwrap();
        assert!((time - 0.00166095).abs() < 0.000001);
        assert_eq!(intensity, 7.7457031249999995);

        let mut n_mzs = 1;
        while reader.next()?.is_some() {
            n_mzs += 1;
        }
        assert_eq!(n_mzs, 12000);
        Ok(())
    }
}
