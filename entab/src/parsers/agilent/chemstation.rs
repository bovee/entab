use alloc::collections::BTreeMap;
use alloc::str;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use core::marker::Copy;

use chrono::NaiveDateTime;

use crate::parsers::agilent::read_agilent_header;
use crate::parsers::{extract, Endian, FromSlice};
use crate::record::{StateMetadata, Value};
use crate::EtError;
use crate::{impl_reader, impl_record};

const CHEMSTATION_TIME_STEP: f64 = 0.2;

#[derive(Clone, Debug, Default)]
/// Metadata consistly found in Chemstation file formats
pub struct ChemstationMetadata {
    /// Time the run started (minutes)
    pub start_time: f64,
    /// Time the ended started (minutes)
    pub end_time: f64,
    /// Name of the signal record (specifically used for e.g. MWD traces)
    pub signal_name: String,
    /// Absolute correction to be applied to all data points
    pub offset_correction: f64,
    /// Scaling correction to be applied to all data points
    pub mult_correction: f64,
    /// In what order this run was performed
    pub sequence: u16,
    /// The vial number this run was performed from
    pub vial: u16,
    /// The replicate number of this run
    pub replicate: u16,
    /// The name of the sample
    pub sample: String,
    /// The description of the sample
    pub description: String,
    /// The name of the operator
    pub operator: String,
    /// The date the sample was run
    pub run_date: Option<NaiveDateTime>,
    /// The instrument the sample was run on
    pub instrument: String,
    /// The method the instrument ran
    pub method: String,
}

impl<'r> From<&ChemstationMetadata> for BTreeMap<String, Value<'r>> {
    fn from(metadata: &ChemstationMetadata) -> Self {
        let mut map = BTreeMap::new();
        drop(map.insert("start_time".to_string(), metadata.start_time.into()));
        drop(map.insert("end_time".to_string(), metadata.end_time.into()));
        drop(map.insert(
            "signal_name".to_string(),
            metadata.signal_name.clone().into(),
        ));
        drop(map.insert(
            "offset_correction".to_string(),
            metadata.offset_correction.into(),
        ));
        drop(map.insert(
            "mult_correction".to_string(),
            metadata.mult_correction.into(),
        ));
        drop(map.insert("sequence".to_string(), metadata.sequence.into()));
        drop(map.insert("vial".to_string(), metadata.vial.into()));
        drop(map.insert("replicate".to_string(), metadata.replicate.into()));
        drop(map.insert("sample".to_string(), metadata.sample.clone().into()));
        drop(map.insert(
            "description".to_string(),
            metadata.description.clone().into(),
        ));
        drop(map.insert("operator".to_string(), metadata.operator.clone().into()));
        drop(map.insert("run_date".to_string(), metadata.run_date.into()));
        drop(map.insert("instrument".to_string(), metadata.instrument.clone().into()));
        drop(map.insert("method".to_string(), metadata.method.clone().into()));
        map
    }
}

fn get_metadata(header: &[u8]) -> Result<ChemstationMetadata, EtError> {
    if header.len() < 648 {
        return Err(
            EtError::from("Chemstation header needs to be at least 648 bytes long").incomplete(),
        );
    }
    let start_time = f64::from(i32::extract(&header[282..], &Endian::Big)?) / 60000.;
    let end_time = f64::from(i32::extract(&header[286..], &Endian::Big)?) / 60000.;

    let offset_correction = f64::extract(&header[636..], &Endian::Big)?;
    let mult_correction = f64::extract(&header[644..], &Endian::Big)?;

    let signal_name_len = usize::from(header[596]);
    if signal_name_len > 40 {
        return Err("Invalid signal name length".into());
    }
    let signal_name = str::from_utf8(&header[597..597 + signal_name_len])?
        .trim()
        .to_string();

    let sample_len = usize::from(header[24]);
    if sample_len > 60 {
        return Err("Invalid sample length".into());
    }
    let sample = str::from_utf8(&header[25..25 + sample_len])?
        .trim()
        .to_string();
    let description_len = usize::from(header[86]);
    if description_len > 60 {
        return Err("Invalid sample length".into());
    }
    let description = str::from_utf8(&header[87..87 + description_len])?
        .trim()
        .to_string();
    let operator_len = usize::from(header[148]);
    if operator_len > 28 {
        return Err("Invalid sample length".into());
    }
    let operator = str::from_utf8(&header[149..149 + operator_len])?
        .trim()
        .to_string();
    let run_date_len = usize::from(header[178]);
    if run_date_len > 60 {
        return Err("Invalid sample length".into());
    }
    // We need to detect the date format before we can convert into a
    // NaiveDateTime; not sure the format even maps to the file type
    // (it may be computer-dependent?)
    let raw_run_date = str::from_utf8(&header[179..179 + run_date_len])?.trim();
    let run_date = if let Ok(d) = NaiveDateTime::parse_from_str(raw_run_date, "%d-%b-%y, %H:%M:%S")
    {
        // format in MWD
        Some(d)
    } else if let Ok(d) = NaiveDateTime::parse_from_str(raw_run_date, "%d %b %y %l:%M %P") {
        // format in MS
        Some(d)
    } else if let Ok(d) = NaiveDateTime::parse_from_str(raw_run_date, "%d %b %y %l:%M %P %z") {
        // format in MS with timezone
        Some(d)
    } else if let Ok(d) = NaiveDateTime::parse_from_str(raw_run_date, "%m/%d/%y %I:%M:%S %p") {
        // format in FID
        Some(d)
    } else {
        None
    };

    let instrument_len = usize::from(header[208]);
    let instrument = str::from_utf8(&header[209..209 + instrument_len])?
        .trim()
        .to_string();
    let method_len = usize::from(header[228]);
    let method = str::from_utf8(&header[229..229 + method_len])?
        .trim()
        .to_string();

    // not sure how robust the following are
    let sequence = u16::extract(&header[252..], &Endian::Big)?;
    let vial = u16::extract(&header[254..], &Endian::Big)?;
    let replicate = u16::extract(&header[256..], &Endian::Big)?;

    Ok(ChemstationMetadata {
        start_time,
        end_time,
        signal_name,
        offset_correction,
        mult_correction,
        sequence,
        vial,
        replicate,
        sample,
        description,
        operator,
        run_date,
        instrument,
        method,
    })
}

#[derive(Clone, Debug, Default)]
/// Internal state for the ChemstationFid parser
pub struct ChemstationFidState {
    cur_time: f64,
    cur_delta: f64,
    cur_intensity: f64,
    time_step: f64,
    metadata: ChemstationMetadata,
}

impl StateMetadata for ChemstationFidState {
    fn metadata(&self) -> BTreeMap<String, Value> {
        (&self.metadata).into()
    }

    fn header(&self) -> Vec<&str> {
        vec!["time", "intensity"]
    }
}

impl<'b: 's, 's> FromSlice<'b, 's> for ChemstationFidState {
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
        let metadata = get_metadata(rb)?;
        // offset the current time back one step so it'll be right after the first time that parse
        self.cur_time = metadata.start_time - CHEMSTATION_TIME_STEP;
        self.cur_intensity = 0.;
        self.cur_delta = 0.;
        self.time_step = CHEMSTATION_TIME_STEP;
        self.metadata = metadata;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default)]
/// A point in a FID trace
pub struct ChemstationFidRecord {
    /// The time recorded at
    pub time: f64,
    /// The intensity recorded
    pub intensity: f64,
}

impl_record!(ChemstationFidRecord: time, intensity);

impl<'b: 's, 's> FromSlice<'b, 's> for ChemstationFidRecord {
    type State = ChemstationFidState;

    fn parse(
        buffer: &[u8],
        eof: bool,
        consumed: &mut usize,
        state: &mut Self::State,
    ) -> Result<bool, EtError> {
        let con = &mut 0;
        if buffer.is_empty() && eof {
            return Ok(false);
        } else if buffer.len() == 1 && eof {
            return Err("FID record was incomplete".into());
        } else if buffer.len() < 2 {
            return Err(EtError::from("Incomplete FID file").incomplete());
        }

        let intensity: i16 = extract(buffer, con, &mut Endian::Big)?;
        if intensity == 32767 {
            let high_value: i32 = extract(buffer, con, &mut Endian::Big)?;
            let low_value: u16 = extract(buffer, con, &mut Endian::Big)?;
            state.cur_delta = 0.;
            state.cur_intensity = f64::from(high_value) * 65534. + f64::from(low_value);
        } else {
            state.cur_delta += f64::from(intensity);
            state.cur_intensity += state.cur_delta;
        }

        state.cur_time += state.time_step;
        *consumed += *con;
        Ok(true)
    }

    fn get(&mut self, _buf: &'b [u8], state: &'s Self::State) -> Result<(), EtError> {
        self.time = state.cur_time;
        self.intensity =
            state.cur_intensity * state.metadata.mult_correction + state.metadata.offset_correction;
        Ok(())
    }
}

#[derive(Clone, Debug, Default)]
/// Internal state for the ChemstationMs parser
pub struct ChemstationMsState {
    n_scans_left: usize,
    n_mzs_left: usize,
    cur_time: f64,
    cur_mz: f64,
    cur_intensity: f64,
    metadata: ChemstationMetadata,
}

impl StateMetadata for ChemstationMsState {
    fn metadata(&self) -> BTreeMap<String, Value> {
        (&self.metadata).into()
    }

    fn header(&self) -> Vec<&str> {
        vec!["time", "mz", "intensity"]
    }
}

impl<'b: 's, 's> FromSlice<'b, 's> for ChemstationMsState {
    type State = ();

    fn parse(
        buffer: &[u8],
        _eof: bool,
        consumed: &mut usize,
        _state: &mut Self::State,
    ) -> Result<bool, EtError> {
        *consumed += read_agilent_header(buffer, true)?;
        Ok(true)
    }

    fn get(&mut self, buffer: &'b [u8], _state: &'s Self::State) -> Result<(), EtError> {
        let metadata = get_metadata(buffer)?;
        let n_scans = u32::extract(&buffer[278..], &Endian::Big)? as usize;

        self.n_scans_left = n_scans;
        self.metadata = metadata;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default)]
/// A single time/mz record from a ChemstationMs file
pub struct ChemstationMsRecord {
    /// The time recorded at
    pub time: f64,
    /// The m/z recorded at
    pub mz: f64,
    /// The intensity recorded
    pub intensity: f64,
}

impl_record!(ChemstationMsRecord: time, mz, intensity);

impl<'b: 's, 's> FromSlice<'b, 's> for ChemstationMsRecord {
    type State = ChemstationMsState;

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
        let mut n_mzs_left = state.n_mzs_left;
        while n_mzs_left == 0 {
            // handle the record header
            let raw_n_mzs_left: u16 = extract(rb, con, &mut Endian::Big)?;
            if raw_n_mzs_left < 14 {
                return Err("Invalid Chemstation MS record header".into());
            }
            n_mzs_left = usize::from((raw_n_mzs_left - 14) / 2);
            state.cur_time = f64::from(extract::<u32>(rb, con, &mut Endian::Big)?) / 60000.;
            // eight more bytes of unknown information and then last 4 bytes
            // is a u16/u16 pair for the highest peak?
            let _ = extract::<&[u8]>(rb, con, &mut 12)?;
            if n_mzs_left == 0 {
                // this is an empty record so debit and eat the footer too
                state.n_scans_left -= 1;
                let _ = extract::<&[u8]>(rb, con, &mut 10)?;
                if state.n_scans_left == 0 {
                    return Ok(false);
                }
            }
        }

        // just read the mz/intensity
        state.cur_mz = f64::from(extract::<u16>(rb, con, &mut Endian::Big)?) / 20.;
        let raw_intensity: u16 = extract(rb, con, &mut Endian::Big)?;
        state.cur_intensity =
            f64::from(raw_intensity & 16383) * 8f64.powi(i32::from(raw_intensity) >> 14);
        if n_mzs_left == 1 {
            state.n_scans_left -= 1;
            // eat the footer
            let _ = extract::<&[u8]>(rb, con, &mut 10)?;
            // the very last 4 bytes are a u32 for the TIC
        }
        state.n_mzs_left = n_mzs_left - 1;

        *consumed += *con;
        Ok(true)
    }

    fn get(&mut self, _buf: &'b [u8], state: &'s Self::State) -> Result<(), EtError> {
        self.time = state.cur_time;
        self.mz = state.cur_mz;
        self.intensity = state.cur_intensity;
        Ok(())
    }
}

#[derive(Clone, Debug, Default)]
/// Internal state for the ChemstationMwd parser
pub struct ChemstationMwdState {
    n_wvs_left: usize,
    cur_time: f64,
    cur_intensity: f64,
    time_step: f64,
    metadata: ChemstationMetadata,
}

impl StateMetadata for ChemstationMwdState {
    fn metadata(&self) -> BTreeMap<String, Value> {
        (&self.metadata).into()
    }

    fn header(&self) -> Vec<&str> {
        vec!["time", "signal", "intensity"]
    }
}

impl<'b: 's, 's> FromSlice<'b, 's> for ChemstationMwdState {
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

    fn get(&mut self, buf: &'b [u8], _state: &'s Self::State) -> Result<(), EtError> {
        let metadata = get_metadata(buf)?;

        self.n_wvs_left = 0;
        // offset the current time back one step so it'll be right after the first time that parse
        self.cur_time = metadata.start_time - CHEMSTATION_TIME_STEP;
        self.cur_intensity = 0.;
        self.time_step = CHEMSTATION_TIME_STEP;
        self.metadata = metadata;
        Ok(())
    }
}

#[derive(Clone, Debug, Default)]
/// A single point from an e.g. moving wavelength detector trace
pub struct ChemstationMwdRecord<'r> {
    /// The name of the signal that's being tracked
    pub signal_name: &'r str,
    /// The time recorded at
    pub time: f64,
    /// The intensity recorded
    pub intensity: f64,
}

impl<'r> From<ChemstationMwdRecord<'r>> for Vec<Value<'r>> {
    fn from(record: ChemstationMwdRecord<'r>) -> Self {
        // signal name is something like "MWD A, Sig=210,5 Ref=360,100"
        let signal = record
            .signal_name
            .split_once("Sig=")
            .map(|x| x.1)
            .and_then(|last_part| {
                Some(last_part.split_once(',').map_or(last_part, |x| x.0))
                    .and_then(|sig_name| sig_name.parse::<f64>().ok())
            })
            .unwrap_or(0.);
        vec![record.time.into(), signal.into(), record.intensity.into()]
    }
}

impl<'b: 's, 's> FromSlice<'b, 's> for ChemstationMwdRecord<'s> {
    type State = ChemstationMwdState;

    fn parse(
        rb: &[u8],
        eof: bool,
        consumed: &mut usize,
        state: &mut Self::State,
    ) -> Result<bool, EtError> {
        if rb.is_empty() && eof {
            return Ok(false);
        }
        let con = &mut 0;
        let mut n_wvs_left = state.n_wvs_left;
        if n_wvs_left == 0 {
            // mask out the top nibble because it's always 0b0001 (i hope?)
            n_wvs_left = usize::from(extract::<u16>(rb, con, &mut Endian::Big)?) & 0b1111_1111_1111;
            if n_wvs_left == 0 {
                // TODO: consume the rest of the file so this can't accidentally repeat?
                return Ok(false);
            }
        }

        let intensity: i16 = extract(rb, con, &mut Endian::Big)?;
        if intensity == -32768 {
            state.cur_intensity = f64::from(extract::<i32>(rb, con, &mut Endian::Big)?);
        } else {
            state.cur_intensity += f64::from(intensity);
        }
        state.n_wvs_left = n_wvs_left - 1;
        state.cur_time += state.time_step;

        *consumed += *con;
        Ok(true)
    }

    fn get(&mut self, _rb: &'b [u8], state: &'s Self::State) -> Result<(), EtError> {
        self.signal_name = &state.metadata.signal_name;
        self.time = state.cur_time;
        self.intensity =
            state.cur_intensity * state.metadata.mult_correction + state.metadata.offset_correction;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default)]
/// Internal state for the ChemstationUv parser
pub struct ChemstationUvState {
    n_scans_left: usize,
    n_wvs_left: usize,
    cur_time: f64,
    cur_intensity: f64,
    cur_wv: f64,
    wv_step: f64,
}

impl StateMetadata for ChemstationUvState {
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

        // TODO: get other metadata
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
/// A record from a ChemstationUv file
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
            let _ = extract::<&[u8]>(rb, con, &mut 4)?;
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
            state.cur_wv = f64::from(wv_start) / 20.;
            state.wv_step = f64::from(wv_step) / 20.;
            let _ = extract::<&[u8]>(rb, con, &mut 8)?;
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
        *consumed += *con;
        Ok(true)
    }

    fn get(&mut self, _rb: &'b [u8], state: &'s Self::State) -> Result<(), EtError> {
        self.time = state.cur_time;
        self.wavelength = state.cur_wv;
        self.intensity = state.cur_intensity / 2000.;
        Ok(())
    }
}

// scratch with offsets for info in different files

// FID - 02 38 31 00 ("81") (missing 01 38 00 00)
// MWD - 02 33 30 00 ("30")
// MS - 01 32 00 00 ("2") (missing 02 32 30?)
// (possibly also 03 31 37 39 and 03 31 38 31 ?)
//  - 5 - "GC / MS Data File" or other?
//  - 24 - Sample Name
//  - 86 - Sample Description?
//  - 148 - Operator Name
//  - 178 - Run Date
//  - 208 - Instrument Name
//  - 218 - LC or GC
//  - 228 - Method Name
//  - 252 - Sequence? (u16)
//  - 254 - Vial? (u16)
//  - 256 - Replicate? (u16)
//  - 260 - TIC Offset? (i32)
//  * 264 - FID/MWD - 512 byte header chunks // 2 + 1
//  - 264 - MS - total header bytes // 2 + 1
//  - 272 - Normalization offset? (i32)
//  * 282 - Start Time (i32)
//  * 286 - End Time (i32)
//  M 322 - Collection software?
//  M 355 - Software Version?
//  - 368 - "GC / MS Data File" as utf16
//  M 405 - Another Version?
//  - 448 - MS - Instrument name as utf16
//  - 530 - lower end of mz/wv range?
//  - 532 - upper end of mz/wv range?
//  - 576 - MS - "GC"
//  - 580 - Units
//  M 596 - Channel Info (str)
//  - 616 - MS - Method directory
//  - 644 - (f32/64?)
//  - 5768 - MS - data start (GC)

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

impl_reader!(
    ChemstationFidReader,
    ChemstationFidRecord,
    ChemstationFidRecord,
    ChemstationFidState,
    ()
);
impl_reader!(
    ChemstationMsReader,
    ChemstationMsRecord,
    ChemstationMsRecord,
    ChemstationMsState,
    ()
);
impl_reader!(
    ChemstationMwdReader,
    ChemstationMwdRecord,
    ChemstationMwdRecord<'r>,
    ChemstationMwdState,
    ()
);
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
    fn test_chemstation_reader_fid() -> Result<(), EtError> {
        let data: &[u8] = include_bytes!("../../../tests/data/test_fid.ch");
        let mut reader = ChemstationFidReader::new(data, None)?;
        let _ = reader.metadata();
        assert_eq!(reader.headers(), ["time", "intensity"]);
        let ChemstationFidRecord { time, intensity } = reader.next()?.unwrap();
        // TODO: try to confirm this time is correct
        assert!((time - 20184.8775).abs() < 0.0001);
        assert!((intensity - 17.500).abs() < 0.001);

        let mut n_mzs = 1;
        while reader.next()?.is_some() {
            n_mzs += 1;
        }
        assert_eq!(n_mzs, 2699);

        Ok(())
    }

    #[test]
    fn test_chemstation_reader_ms() -> Result<(), EtError> {
        let data: &[u8] = include_bytes!("../../../tests/data/carotenoid_extract.d/MSD1.MS");
        let mut reader = ChemstationMsReader::new(data, None)?;
        let _ = reader.metadata();
        assert_eq!(reader.headers(), ["time", "mz", "intensity"]);
        let ChemstationMsRecord {
            time,
            mz,
            intensity,
        } = reader.next()?.unwrap();
        assert!((time - 0.079166).abs() < 0.000001);
        assert!((mz - 915.7).abs() < 0.000001);
        assert_eq!(intensity, 112.);

        let ChemstationMsRecord {
            time,
            mz,
            intensity,
        } = reader.next()?.unwrap();
        assert!((time - 0.079166).abs() < 0.000001);
        assert!((mz - 865.4).abs() < 0.000001);
        assert_eq!(intensity, 184.);

        let mut n_mzs = 2;
        while reader.next()?.is_some() {
            n_mzs += 1;
        }
        assert_eq!(n_mzs, 95471);
        Ok(())
    }

    #[test]
    fn test_chemstation_reader_mwd() -> Result<(), EtError> {
        let data: &[u8] = include_bytes!("../../../tests/data/chemstation_mwd.d/mwd1A.ch");
        let mut reader = ChemstationMwdReader::new(data, None)?;
        assert_eq!(reader.headers(), ["time", "signal", "intensity"]);
        let _ = reader.metadata();
        let ChemstationMwdRecord {
            time,
            signal_name,
            intensity,
        } = reader.next()?.unwrap();
        assert!((time - -0.039667).abs() < 0.000001);
        assert_eq!(signal_name, "MWD A, Sig=210,5 Ref=360,100");
        assert!((intensity - -36.34977).abs() < 0.00001);

        let mut n_mzs = 1;
        while reader.next()?.is_some() {
            n_mzs += 1;
        }
        assert_eq!(n_mzs, 1801);
        Ok(())
    }

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
        assert_eq!(intensity, -15.6675);

        let mut n_mzs = 1;
        while reader.next()?.is_some() {
            n_mzs += 1;
        }
        assert_eq!(n_mzs, 6744 * 301);
        Ok(())
    }

    #[test]
    fn test_chemstation_reader_bad_fuzzes() -> Result<(), EtError> {
        let test_data = b"\x012>\n\n\n\n\n\n>*\n\x86\n>\n\n\n\n\n\n\n\n\x14\n\n\n\n\n\n\n\n\xaf%\xa8\x00\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\n\n\n\n\n\n\n\n\n\n\n\n\n>>>\n*\n\n>>\n\xe3\x86\x86\n>>\n\n\n\n>\n\n\n\xaf%\x00\x00\x00\x00\x00\x00\x01\x04\n\n\n\n\n\n\n\n\n\n\n\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\n\n\n\n\n\n\n\n\n\n\n\n\n\n>>>\n*\n\n>>>\n\n\n\n>\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\n\n\n\n\n\n\n\n>\n\n\n\n>";
        assert!(ChemstationMsReader::new(&test_data[..], None).is_err());

        let test_data = b"\x012>\n\n\n\n\n\n>*\n\x86\n>\n\n>\n\xE3\x86\n>\n>\n\n>\n\xE3\x86&\n>@\x10\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\n\n\n\n\n\n\n\n\n\n\n\n\n\n\x02Y\n\n\n\n\xE3\x86\x86\n>\n\n>\n\n>\n*\n\n\n>\n\n>\n\n>\n\xE3\n\n\n\n\n\n\x14\n\n\n\n>\n\xC8>\n\x86\n>\n\n\n\n\n\n\n\n\n\n\n\n>\n\xE3\xCD\xCD\xCD\x00\x00\n\n\n\n\n\n>\n\n>\n\x00\n\x00\n\n\n\n\n\n\n>\n\n\n>\n\n\n\n\n\n\n\n>\n\n\n\n\n\n>\n\n\n\n\x00\x00\n\n\n\x00\n\n\n\n\n\n\n\n\xE3\x00\x00\n\n\n\n\n>\n\n\n>\n\n\n\n\n\n\n\n>\n\n\n\n\n\n>\n\n\n\n\x00\x00\n\n\n\x00\n\n\n\n\n\n\n\n\xE3\x00\x00\x00>\x0b\n\x01\x00>\n\n\n\x00>\n\n\x01\x00>\n\n\n\n\x00\x00\n\n\n\x00\n\n\n\n\n>\n\n>\n\n\n\n\n\n\n\n\n\n\n\n\x02Y\n\n\n\n\xE3\x86\n>>*\n\x86\xE3\x86\n>>*\n\x86\x00R>N\x02\xE3\n>\n>\xC6\n\n>\n\xE3\x00\x00\x00\x00\x00\x00\n\n\n\n\n>\n\xE3\xCD\n>\n\n>\n\xE3\n>@W\n\n+\n\n\n>\n\n>\n\xE3>*\n\x86*\n\x86\xE3\x86\n>>*\n\x86\xE3\x86\n>>*\n\x86\x00R>N\x02\xE3\n>\n>\xC6\n\n>\n\xE3\x00\x00\x00\n\n\n\n\n\n\n\n\n\n\n\n\n\x02Y\n\n\n\n\xE3\x86\x86\n>\n\n>\n\n>\n*\n\n\n>\n\n>\n\n\n\n\n\n\n\n\n\n\n\n\x02Y\n\n\n\n\xE3\x86\x86\n>\n\n>\x01\x00\x00\x00\x00\x00\x00\x01>\n\n>\n\n>\n\xE3\n\n\n\n\n\x01\x00\x00\x00\x00\x00\x00\x00\n\xE3\n>@W>N\x02\xE3\n>\n>\xC6\n\n>\n\xE3\x00\x00\x00";
        assert!(ChemstationMsReader::new(&test_data[..], None).is_err());

        let test_data = b"\x012>\n\n\n\n\n\n\n\n\n\n\n\n\n\n\x14\n\n\n\n\n\n\n\n\xAF%\xA8\x00\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\nVVVVV\n\n\xAF%\xA8\x00\xFE\x00\x00\x00\x00\x00\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x80\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x08\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x00\x00\x00\x00\x02\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x02\x00\n\n\n\n\n\n\n\n\n>";
        assert!(ChemstationMsReader::new(&test_data[..], None).is_err());

        let test_data = [
            1, 50, 0, 0, 62, 14, 14, 14, 14, 14, 14, 14, 14, 65, 14, 14, 14, 14, 14, 14, 14, 14,
            14, 14, 14, 18, 14, 14, 14, 14, 14, 14, 255, 255, 255, 255, 10, 255, 255, 255, 255,
            255, 10, 10, 147, 245, 62, 116, 116, 80, 71, 80, 80, 80, 80, 80, 80, 80, 80, 80, 26,
            75, 212, 213, 0, 1, 0, 0, 0, 0, 0, 0, 6, 0, 0, 0, 1, 33, 36, 116, 116, 116, 116, 116,
            116, 116, 118, 116, 116, 116, 255, 255, 255, 255, 116, 116, 116, 116, 116, 14, 14, 14,
            14, 14, 54, 54, 54, 54, 44, 10, 255, 255, 255, 255, 255, 0, 0, 0, 0, 4, 3, 2, 1, 83,
            80, 65, 72, 66, 65, 77, 255, 255, 255, 255, 255, 1, 83, 80, 65, 72, 66, 65, 77, 255,
            255, 255, 255, 255, 255, 255, 255, 0, 244, 10, 255, 10, 0, 14, 54, 54, 54, 54, 54, 54,
            54, 54, 54, 53, 5, 5, 10, 62, 44, 10, 255, 255, 255, 255, 255, 0, 0, 0, 0, 4, 3, 2, 1,
            83, 80, 65, 72, 66, 65, 77, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 0, 244,
            10, 255, 10, 0, 0, 4, 3, 2, 255, 255, 255, 255, 0, 244, 10, 255, 10, 0, 0, 4, 3, 2,
            255, 255, 255, 255, 0, 244, 10, 255, 10, 0, 0, 4, 3, 2, 255, 255, 255, 255, 0, 244, 10,
            255, 10, 0, 4, 3, 2, 255, 255, 255, 255, 0, 244, 10, 255, 10, 0, 0, 4, 3, 2, 10, 255,
            10, 0,
        ];
        assert!(ChemstationMsReader::new(&test_data[..], None).is_err());

        let test_data = [
            1, 50, 0, 0, 62, 14, 14, 14, 14, 14, 14, 14, 14, 65, 14, 14, 14, 14, 14, 14, 14, 14,
            14, 14, 14, 18, 14, 243, 1, 0, 0, 0, 0, 0, 0, 14, 14, 14, 14, 14, 14, 14, 14, 14, 65,
            14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 18, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14,
            14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 62, 10, 10, 10, 10, 10, 10, 10, 10,
            116, 10, 62, 116, 116, 80, 80, 80, 80, 80, 80, 80, 80, 80, 80, 80, 80, 80, 80, 80, 0,
            1, 0, 0, 0, 0, 0, 0, 6, 0, 0, 0, 1, 33, 36, 116, 116, 116, 116, 116, 116, 116, 118,
            116, 116, 116, 255, 255, 255, 255, 116, 124, 116, 116, 116, 14, 14, 14, 14, 14, 48, 55,
            52, 53, 49, 52, 56, 50, 54, 48, 5, 5, 10, 62, 44, 10, 255, 255, 255, 255, 255, 0, 0, 0,
            0, 0, 244, 185, 251, 222, 252, 255, 255, 255, 255, 255, 255, 0, 0, 0, 0, 14, 14, 14,
            14, 14, 14, 14, 14, 14, 14, 14, 255, 255, 255, 255, 255, 255, 0, 58, 10, 10, 10, 10,
            147, 245, 62, 116, 116, 80, 80, 14, 14, 14, 14, 14, 14, 62, 10, 10, 10, 10, 10, 10, 10,
            10, 116, 10, 62, 116, 116, 80, 80, 80, 80, 80, 80, 80, 80, 80, 80, 80, 80, 80, 80, 80,
            0, 1, 0, 0, 0, 0, 0, 0, 6, 0, 0, 0, 1, 33, 36, 116, 116, 116, 116, 116, 116, 116, 118,
            116, 116, 116, 255, 255, 255, 255, 116, 124, 116, 116, 116, 14, 14, 14, 14, 14, 48, 55,
            52, 53, 49, 52, 56, 50, 54, 48, 5, 5, 10, 62, 44, 10, 255, 255, 255, 255, 255, 0, 0, 0,
            0, 0, 244, 185, 251, 222, 252, 255, 255, 255, 255, 255, 255, 0, 0, 0, 0, 14, 14, 14,
            14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 62, 10, 10, 14, 10, 10, 10, 10, 10, 10, 147,
            245, 62, 116, 116, 80, 80, 36, 116, 116, 116, 116, 116, 116, 116, 118, 116, 116, 116,
            255, 255, 255, 255, 116, 116, 116, 116, 116, 14, 14, 14, 14, 14, 54, 54, 54, 54, 54,
            54, 54, 54, 54, 53, 5, 5, 10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 4, 62, 1, 255, 255,
            255, 255, 255, 255, 0, 152, 0, 10, 10, 10, 10, 10, 10, 10, 62, 10, 116, 13, 1, 0, 0,
            97, 115, 116, 97, 118, 116, 116, 116, 255, 255, 255, 255, 116, 116, 116, 116, 116, 116,
            116, 0, 0, 0, 0, 0, 10, 62, 10, 10, 10, 246, 245, 245, 240, 1, 0, 0, 0, 0, 0, 0, 0,
            255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 175, 255,
            255, 255, 255, 255, 255, 255, 255, 0, 0, 0, 9, 255, 255, 255, 255, 255, 255, 255, 255,
            255, 255, 255, 116, 116, 118, 116, 116, 116, 255, 255, 255, 248, 10, 45, 26, 244, 10,
            62, 8, 10, 208, 255, 255, 255, 255, 255, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5,
            116, 0, 116, 116, 116, 116, 116, 0, 0, 0, 46, 0, 0, 0, 0, 0, 0, 0, 116, 0, 0, 0, 0,
            116, 116, 0, 0, 116, 0, 0,
        ];
        assert!(ChemstationMsReader::new(&test_data[..], None).is_err());

        let test_data = [
            1, 50, 0, 0, 62, 14, 14, 14, 14, 14, 14, 14, 14, 65, 14, 14, 14, 14, 14, 14, 14, 14,
            14, 26, 0, 0, 0, 0, 0, 0, 0, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14, 14,
            14, 14, 62, 10, 10, 10, 10, 100, 10, 10, 10, 10, 116, 10, 62, 57, 2, 80, 80, 80, 80,
            80, 80, 80, 80, 80, 80, 80, 80, 80, 80, 80, 0, 1, 255, 62, 1, 0, 0, 0, 0, 254, 254,
            254, 254, 168, 0, 0, 0, 0, 168, 168, 168, 168, 168, 168, 168, 168, 168, 168, 168, 168,
            224, 7, 168, 168, 168, 169, 168, 168, 168, 168, 168, 168, 168, 168, 168, 168, 54, 54,
            54, 53, 5, 5, 10, 62, 44, 10, 255, 255, 255, 255, 255, 0, 0, 0, 0, 0, 244, 185, 251,
            222, 252, 255, 255, 255, 255, 255, 255, 0, 0, 0, 0, 14, 14, 14, 14, 14, 14, 14, 14, 14,
            14, 14, 14, 14, 62, 10, 10, 14, 10, 10, 10, 10, 10, 10, 147, 245, 62, 116, 116, 80, 80,
            80, 80, 80, 80, 80, 80, 80, 80, 80, 26, 75, 212, 213, 0, 1, 0, 0, 0, 67, 0, 6, 0, 0,
            70, 0, 83, 51, 46, 49, 32, 0, 0, 1, 33, 36, 116, 116, 116, 116, 116, 116, 116, 118,
            116, 116, 116, 255, 255, 255, 255, 116, 116, 116, 116, 116, 80, 80, 80, 80, 80, 80, 80,
            26, 75, 212, 213, 0, 1, 0, 0, 0, 0, 0, 0, 6, 0, 0, 0, 1, 116, 116, 116, 116, 116, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 5, 116, 116, 116, 33, 36, 116, 116, 116, 116, 255, 31, 1,
            0, 255, 255, 65, 0, 0, 0, 0, 0, 0, 245, 10, 10, 10, 10, 10, 10, 62, 10, 116, 116, 116,
            116, 116, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 5, 116, 116, 116, 116, 3, 116, 246, 245,
            245, 240, 10, 62, 8, 10, 255, 255, 255, 255, 255, 255, 185, 255, 255, 255, 255, 255,
            255, 10, 10, 10, 10, 0, 0, 0, 0, 0, 0, 10, 1, 255, 10, 10, 10, 62, 10, 9, 9, 9, 255,
            10, 10, 10, 62, 10, 10, 135, 0, 0, 0, 0, 0, 8, 201, 64, 248, 181, 42, 124, 255, 255,
            255, 10, 10, 10, 10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 57, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 62, 1, 0, 0, 0, 0, 0, 7, 0, 0, 0, 10, 10, 10, 116, 116, 116, 116, 116, 116, 118,
            116, 116, 116, 255, 255, 31, 0, 3, 219, 116, 116, 116, 116, 246, 245, 245, 240, 10, 62,
            50, 10, 255, 187, 255, 255, 251, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
            255, 255, 10, 10, 10, 62, 116, 116, 116, 116, 116, 116, 116, 116, 116, 118, 116, 116,
            116, 255, 255, 255, 255, 116, 116, 116, 116, 116, 116, 246, 245, 245, 76, 10, 62, 8,
            10, 255, 255, 32, 32, 0, 3, 219, 30, 30, 31, 1, 0, 255, 255, 65, 0, 0, 0, 0, 0, 0, 245,
            10, 10, 10, 10, 0, 0, 0, 5, 116, 116, 116, 116, 3, 116, 246, 245, 245, 240, 10, 62, 8,
            10, 255, 255, 255, 251, 255, 255, 255, 255, 255, 255, 255, 255, 255, 10, 10, 10, 10, 0,
            0, 0, 0, 0, 0, 10, 1, 255, 10, 10, 10, 62, 10, 9, 9, 9, 255, 10, 10, 10, 62, 10, 10,
            135, 0, 0, 0, 0, 0, 8, 201, 64, 248, 181, 42, 124, 0, 0, 0, 0, 245, 10, 10, 10, 10, 10,
            10, 62, 10, 116, 116, 116, 116, 116, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 5, 116, 116,
            116, 116, 3, 116, 246, 245, 245, 240, 10, 62, 8, 10, 255, 255, 255, 251, 255, 255, 255,
            255, 255, 255, 255, 255, 255, 10, 10, 10, 10, 0, 0, 0, 0, 0, 0, 10, 1, 255, 10, 10, 10,
            62, 10, 9, 9, 9, 255, 10, 10, 10, 62, 10, 10, 135, 0, 0, 0, 0, 0, 8, 201, 64, 248, 181,
            42, 124, 10, 10, 62, 10, 10, 135, 0, 0, 0, 0, 0, 8, 201, 64, 248, 181, 42, 124,
        ];
        assert!(ChemstationMsReader::new(&test_data[..], None).is_err());

        Ok(())
    }
}
