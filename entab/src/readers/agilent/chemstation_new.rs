use core::marker::Copy;

use crate::buffer::ReadBuffer;
use crate::parsers::{Endian, FromBuffer, FromSlice};
use crate::readers::agilent::read_agilent_header;
use crate::record::StateMetadata;
use crate::EtError;
use crate::{impl_reader, impl_record};

/// Internal state for the ChemstationUv parser
#[derive(Clone, Copy, Debug, Default)]
pub struct ChemstationUvState {
    n_scans_left: usize,
    n_wvs_left: usize,
    cur_time: f64,
    cur_intensity: f64,
    cur_wv: f64,
    wv_step: f64,
}

impl<'r> StateMetadata<'r> for ChemstationUvState {}

impl<'r> FromBuffer<'r> for ChemstationUvState {
    type State = ();

    fn from_buffer(
        &mut self,
        mut rb: &'r mut ReadBuffer,
        _state: Self::State,
    ) -> Result<bool, EtError> {
        let header = read_agilent_header(&mut rb, false)?;
        let n_scans = u32::out_of(&header[278..], Endian::Big)? as usize;

        // TODO: get other metadata
        self.n_scans_left = n_scans;
        self.n_wvs_left = 0;
        self.cur_time = 0.;
        self.cur_wv = 0.;
        self.cur_intensity = 0.;
        self.wv_step = 0.;
        Ok(true)
    }
}

/// A record from a ChemstationUv file
#[derive(Clone, Copy, Debug, Default)]
pub struct ChemstationUvRecord {
    /// The time recorded at
    pub time: f64,
    /// The wavelength recorded at
    pub wavelength: f64,
    /// The intensity record
    pub intensity: f64,
}

impl_record!(ChemstationUvRecord: time, wavelength, intensity);

impl<'r> FromBuffer<'r> for ChemstationUvRecord {
    type State = &'r mut ChemstationUvState;

    fn from_buffer(&mut self, rb: &'r mut ReadBuffer, state: Self::State) -> Result<bool, EtError> {
        if state.n_scans_left == 0 {
            return Ok(false);
        }

        // refill case
        if state.n_wvs_left == 0 {
            let _ = rb.extract::<&[u8]>(4_usize)?;
            // let next_pos = usize::from(rb.extract::<u16>(Endian::Little)?);
            state.cur_time = (rb.extract::<u32>(Endian::Little)? as f64) / 60000.;
            let wv_start: u16 = rb.extract(Endian::Little)?;
            let wv_end: u16 = rb.extract(Endian::Little)?;
            let wv_step: u16 = rb.extract(Endian::Little)?;

            state.n_wvs_left = usize::from((wv_end - wv_start) / wv_step) + 1;
            state.cur_wv = f64::from(wv_start) / 20.;
            state.wv_step = f64::from(wv_step) / 20.;
            let _ = rb.extract::<&[u8]>(8_usize)?;
        };

        let delta = rb.extract::<i16>(Endian::Little)?;
        if delta == -32768 {
            state.cur_intensity = f64::from(rb.extract::<u32>(Endian::Little)?);
        } else {
            state.cur_intensity += f64::from(delta);
        }

        if state.n_wvs_left == 1 {
            state.n_scans_left -= 1;
        }
        state.n_wvs_left -= 1;

        self.time = state.cur_time;
        self.wavelength = state.cur_wv;
        self.intensity = state.cur_intensity / 2000.;
        Ok(true)
    }
}

impl_reader!(
    /// A reader for a Chemstation UV file
    ChemstationUvReader,
    ChemstationUvRecord,
    ChemstationUvState,
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
    use crate::buffer::ReadBuffer;
    use crate::readers::RecordReader;

    #[test]
    fn test_chemstation_reader_uv() -> Result<(), EtError> {
        let rb = ReadBuffer::from_slice(include_bytes!(
            "../../../tests/data/carotenoid_extract.d/dad1.uv"
        ));
        let mut reader = ChemstationUvReader::new(rb, ())?;
        let _ = reader.metadata();
        assert_eq!(reader.headers(), ["time", "wavelength", "intensity"]);
        let ChemstationUvRecord {
            time,
            wavelength,
            intensity,
        } = reader.next()?.unwrap();
        assert!((time - 0.001333).abs() < 0.000001);
        assert!((wavelength - 200.).abs() < 0.000001);
        assert_eq!(intensity, -15.6675);

        let mut n_mzs = 1;
        while let Some(_) = reader.next()? {
            n_mzs += 1;
        }
        assert_eq!(n_mzs, 6744 * 301);
        Ok(())
    }
}
