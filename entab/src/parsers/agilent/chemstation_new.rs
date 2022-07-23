use alloc::collections::BTreeMap;
use alloc::str;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use core::char::{decode_utf16, REPLACEMENT_CHARACTER};
use core::marker::Copy;

use chrono::NaiveDateTime;

use crate::parsers::agilent::read_agilent_header;
use crate::parsers::{extract, Endian, FromSlice};
use crate::record::{StateMetadata, Value};
use crate::EtError;
use crate::{impl_reader, impl_record};

#[derive(Clone, Debug, Default)]
/// Metadata consistly found in new Chemstation file formats
pub struct ChemstationNewMetadata {
    /// Scaling correction to be applied to all data points
    pub mult_correction: f64,
    /// The name of the sample
    pub sample: String,
    /// The name of the operator
    pub operator: String,
    /// The date the sample was run
    pub run_date: Option<NaiveDateTime>,
    /// The instrument the sample was run on
    pub instrument: String,
    /// The method the instrument ran
    pub method: String,
}

impl<'r> From<&ChemstationNewMetadata> for BTreeMap<String, Value<'r>> {
    fn from(metadata: &ChemstationNewMetadata) -> Self {
        let mut map = BTreeMap::new();
        drop(map.insert(
            "mult_correction".to_string(),
            metadata.mult_correction.into(),
        ));
        drop(map.insert("sample".to_string(), metadata.sample.clone().into()));
        drop(map.insert("operator".to_string(), metadata.operator.clone().into()));
        drop(map.insert("run_date".to_string(), metadata.run_date.into()));
        drop(map.insert("instrument".to_string(), metadata.instrument.clone().into()));
        drop(map.insert("method".to_string(), metadata.method.clone().into()));
        map
    }
}

fn get_utf16_pascal(data: &[u8]) -> String {
    let iter = (1..=2 * usize::from(data[0]))
        .step_by(2)
        .map(|i| u16::from_le_bytes([data[i], data[i + 1]]));
    decode_utf16(iter)
        .map(|r| r.unwrap_or(REPLACEMENT_CHARACTER))
        .collect::<String>()
}

fn get_new_metadata(header: &[u8]) -> Result<ChemstationNewMetadata, EtError> {
    if header.len() < 4000 {
        return Err(
            EtError::from("New chemstation header needs to be at least 4000 bytes long")
                .incomplete(),
        );
    }
    //  Also, @ 3093 - Units?
    let sample = get_utf16_pascal(&header[858..]);
    let operator = get_utf16_pascal(&header[1880..]);
    let instrument = get_utf16_pascal(&header[2492..]);
    let method = get_utf16_pascal(&header[2574..]);
    let mult_correction = f64::extract(&header[3085..3093], &Endian::Big)?;

    // We need to detect the date format before we can convert into a
    // NaiveDateTime; not sure the format even maps to the file type
    // (it may be computer-dependent?)
    let raw_run_date = get_utf16_pascal(&header[2391..]);
    let run_date = if let Ok(d) = NaiveDateTime::parse_from_str(&raw_run_date, "%d-%b-%y, %H:%M:%S")
    {
        // format in MWD
        Some(d)
    } else if let Ok(d) = NaiveDateTime::parse_from_str(&raw_run_date, "%d %b %y %l:%M %P") {
        // format in MS
        Some(d)
    } else if let Ok(d) = NaiveDateTime::parse_from_str(&raw_run_date, "%d %b %y %l:%M %P %z") {
        // format in MS with timezone
        Some(d)
    } else if let Ok(d) = NaiveDateTime::parse_from_str(&raw_run_date, "%m/%d/%y %I:%M:%S %p") {
        // format in FID
        Some(d)
    } else {
        None
    };

    Ok(ChemstationNewMetadata {
        mult_correction,
        sample,
        operator,
        run_date,
        instrument,
        method,
    })
}

#[derive(Clone, Debug, Default)]
/// Internal state for the `ChemstationUvRecord` parser
pub struct ChemstationUvState {
    metadata: ChemstationNewMetadata,
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

        self.metadata = get_new_metadata(rb)?;
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
        if n_wvs_left == 0 {
            let _ = extract::<&[u8]>(rb, con, &mut 4)?; // 67, 624/224
                                                        // let next_pos = usize::from(rb.extract::<u16>(Endian::Little)?);
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
}
