use alloc::vec;
use alloc::vec::Vec;
use core::marker::Copy;

use crate::parsers::agilent::read_agilent_header;
use crate::parsers::{extract, Endian, FromSlice};
use crate::record::StateMetadata;
use crate::EtError;
use crate::{impl_reader, impl_record};

/// Internal state for the ChemstationUv parser
#[derive(Clone, Copy, Debug, Default)]
pub struct ChemstationNewUvState {
    n_scans_left: usize,
    n_wvs_left: usize,
    cur_time: f64,
    cur_intensity: f64,
    cur_wv: f64,
    wv_step: f64,
}

impl StateMetadata for ChemstationNewUvState {
    fn header(&self) -> Vec<&str> {
        vec!["time", "wavelength", "intensity"]
    }
}

impl<'b: 's, 's> FromSlice<'b, 's> for ChemstationNewUvState {
    type State = ();

    fn parse(
        rb: &[u8],
        _eof: bool,
        consumed: &mut usize,
        _state: &mut Self::State,
    ) -> Result<bool, EtError> {
        *consumed += read_agilent_header(rb, false)?;
        // TODO: get other metadata
        Ok(true)
    }

    fn get(&mut self, rb: &'b [u8], _state: &'s Self::State) -> Result<(), EtError> {
        let n_scans = extract::<u32>(&rb[278..], &mut 0, &mut Endian::Big)? as usize;
        self.n_scans_left = n_scans;
        Ok(())
    }
}

/// A record from a ChemstationUv file
#[derive(Clone, Copy, Debug, Default)]
pub struct ChemstationNewUvRecord {
    /// The time recorded at
    pub time: f64,
    /// The wavelength recorded at
    pub wavelength: f64,
    /// The intensity record
    pub intensity: f64,
}

impl_record!(ChemstationNewUvRecord: time, wavelength, intensity);

impl<'b: 's, 's> FromSlice<'b, 's> for ChemstationNewUvRecord {
    type State = ChemstationNewUvState;

    fn parse(
        rb: &[u8],
        _eof: bool,
        _consumed: &mut usize,
        state: &mut Self::State,
    ) -> Result<bool, EtError> {
        if state.n_scans_left == 0 {
            return Ok(false);
        }

        let con = &mut 0;
        // refill case
        let mut n_wvs_left = state.n_wvs_left;
        if n_wvs_left == 0 {
            let _ = extract::<&[u8]>(rb, con, &mut 4_usize)?;
            // let next_pos = usize::from(rb.extract::<u16>(Endian::Little)?);
            state.cur_time = f64::from(extract::<u32>(rb, con, &mut Endian::Little)?) / 60000.;
            let wv_start: u16 = extract(rb, con, &mut Endian::Little)?;
            let wv_end: u16 = extract(rb, con, &mut Endian::Little)?;
            if wv_start > wv_end {
                return Err("Invalid wavelength start and end".into());
            }
            let wv_step: u16 = extract(rb, con, &mut Endian::Little)?;
            if wv_step == 0 {
                return Err("Invalid wavelength step".into());
            }

            n_wvs_left = usize::from((wv_end - wv_start) / wv_step) + 1;
            state.cur_wv = f64::from(wv_start) / 20.;
            state.wv_step = f64::from(wv_step) / 20.;
            let _ = extract::<&[u8]>(rb, con, &mut 8_usize)?;
        };

        let delta = extract::<i16>(rb, con, &mut Endian::Little)?;
        if delta == -32768 {
            state.cur_intensity = f64::from(extract::<u32>(rb, con, &mut Endian::Little)?);
        } else {
            state.cur_intensity += f64::from(delta);
        }

        if state.n_wvs_left == 1 {
            state.n_scans_left -= 1;
        }
        state.n_wvs_left = n_wvs_left - 1;
        Ok(true)
    }

    fn get(&mut self, _rb: &'b [u8], state: &'s Self::State) -> Result<(), EtError> {
        self.time = state.cur_time;
        self.wavelength = state.cur_wv;
        self.intensity = state.cur_intensity / 2000.;
        Ok(())
    }
}

impl_reader!(
    /// A reader for a Chemstation UV file
    ChemstationNewUvReader,
    ChemstationNewUvRecord,
    ChemstationNewUvRecord,
    ChemstationNewUvState,
    ()
);

// LC - 03 31 33 31 ("131")
//  * 264 - 512 byte header chunks // 2 + 1
//  ? 278 - Number of Records
//  - 858 - Sample Name
//  - 1880 - Operator Name
//  - 2391 - Run Date
//  - 2492 - Instrument Name
//  - 2533 - "LC"
//  - 2574 - Method Name
//  - 3093 - Units
//   4096 - data start?

#[cfg(test)]
mod tests {
    use super::*;

    use crate::readers::RecordReader;

    #[test]
    fn test_chemstation_reader_uv() -> Result<(), EtError> {
        let test_data: &[u8] = include_bytes!("../../../tests/data/carotenoid_extract.d/dad1.uv");
        let mut reader = ChemstationNewUvReader::new(test_data, None)?;
        let _ = reader.metadata();
        assert_eq!(reader.headers(), ["time", "wavelength", "intensity"]);
        let ChemstationNewUvRecord {
            time,
            wavelength,
            intensity,
        } = reader.next()?.unwrap();
        assert!((time - 0.001333).abs() < 0.000001);
        assert!((wavelength - 200.).abs() < 0.000001);
        assert_eq!(intensity, -15.6675);

        let mut n_mzs = 1;
        while reader.next()?.is_some() {
            n_mzs += 1;
        }
        assert_eq!(n_mzs, 6744 * 301);
        Ok(())
    }
}
