use alloc::collections::BTreeMap;
use alloc::str;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use core::marker::Copy;

use crate::buffer::ReadBuffer;
use crate::parsers::{Endian, FromBuffer, FromSlice};
use crate::record::{RecordHeader, StateMetadata, Value};
use crate::EtError;
use crate::{impl_reader, impl_record};

fn read_agilent_header<'r>(rb: &'r mut ReadBuffer, ms_format: bool) -> Result<&'r [u8], EtError> {
    rb.reserve(268)?;

    // figure out how big the header should be and then get it
    let raw_header_size = u32::out_of(&rb[264..268], Endian::Big)? as usize;
    if raw_header_size == 0 {
        return Err(EtError::new("Invalid header length of 0"));
    }
    let mut header_size = 2 * (raw_header_size - 1);
    if !ms_format {
        header_size *= 256;
    }
    if header_size < 512 {
        return Err(EtError::new("Header length too short"));
    }
    rb.extract::<&[u8]>(header_size)
}

#[derive(Debug)]
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
    pub run_date: String,
    /// The instrument the sample was run on
    pub instrument: String,
    /// The method the instrument ran
    pub method: String,
}

impl<'r> From<&ChemstationMetadata> for BTreeMap<String, Value<'r>> {
    fn from(metadata: &ChemstationMetadata) -> Self {
        let mut map = BTreeMap::new();
        let _ = map.insert("start_time".to_string(), metadata.start_time.into());
        let _ = map.insert("end_time".to_string(), metadata.end_time.into());
        let _ = map.insert(
            "signal_name".to_string(),
            metadata.signal_name.clone().into(),
        );
        let _ = map.insert(
            "offset_correction".to_string(),
            metadata.offset_correction.into(),
        );
        let _ = map.insert(
            "mult_correction".to_string(),
            metadata.mult_correction.into(),
        );
        let _ = map.insert("sequence".to_string(), metadata.sequence.into());
        let _ = map.insert("vial".to_string(), metadata.vial.into());
        let _ = map.insert("replicate".to_string(), metadata.replicate.into());
        let _ = map.insert("sample".to_string(), metadata.sample.clone().into());
        let _ = map.insert(
            "description".to_string(),
            metadata.description.clone().into(),
        );
        let _ = map.insert("operator".to_string(), metadata.operator.clone().into());
        let _ = map.insert("run_date".to_string(), metadata.run_date.clone().into());
        let _ = map.insert("instrument".to_string(), metadata.instrument.clone().into());
        let _ = map.insert("method".to_string(), metadata.method.clone().into());
        map
    }
}

fn get_metadata(header: &[u8]) -> Result<ChemstationMetadata, EtError> {
    let start_time = f64::from(i32::out_of(&header[282..], Endian::Big)?) / 60000.;
    let end_time = f64::from(i32::out_of(&header[286..], Endian::Big)?) / 60000.;

    let offset_correction = f64::out_of(&header[636..], Endian::Big)?;
    let mult_correction = f64::out_of(&header[644..], Endian::Big)?;

    let signal_name_len = usize::from(header[596]);
    let signal_name = str::from_utf8(&header[597..597 + signal_name_len])?
        .trim()
        .to_string();

    let sample_len = usize::from(header[24]);
    let sample = str::from_utf8(&header[25..25 + sample_len])?
        .trim()
        .to_string();
    let description_len = usize::from(header[86]);
    let description = str::from_utf8(&header[87..87 + description_len])?
        .trim()
        .to_string();
    let operator_len = usize::from(header[148]);
    let operator = str::from_utf8(&header[149..149 + operator_len])?
        .trim()
        .to_string();
    let run_date_len = usize::from(header[178]);
    let run_date = str::from_utf8(&header[179..179 + run_date_len])?
        .trim()
        .to_string();
    let instrument_len = usize::from(header[208]);
    let instrument = str::from_utf8(&header[209..209 + instrument_len])?
        .trim()
        .to_string();
    let method_len = usize::from(header[228]);
    let method = str::from_utf8(&header[229..229 + method_len])?
        .trim()
        .to_string();

    // not sure how robust the following are
    let sequence = u16::out_of(&header[252..], Endian::Big)?;
    let vial = u16::out_of(&header[254..], Endian::Big)?;
    let replicate = u16::out_of(&header[256..], Endian::Big)?;

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

#[derive(Debug)]
/// Internal state for the ChemstationFid parser
pub struct ChemstationFidState {
    cur_time: f64,
    cur_delta: f64,
    cur_intensity: f64,
    time_step: f64,
    metadata: ChemstationMetadata,
}

impl<'r> StateMetadata<'r> for ChemstationFidState {
    fn metadata(&self) -> BTreeMap<String, Value> {
        (&self.metadata).into()
    }
}

impl<'r> FromBuffer<'r> for ChemstationFidState {
    type State = ();

    fn get(mut rb: &'r mut ReadBuffer, _state: Self::State) -> Result<Self, EtError> {
        let header = read_agilent_header(&mut rb, false)?;
        let metadata = get_metadata(&header)?;

        Ok(ChemstationFidState {
            cur_time: metadata.start_time,
            cur_intensity: 0.,
            cur_delta: 0.,
            time_step: 0.2,
            metadata,
        })
    }
}

#[derive(Clone, Copy, Debug)]
/// A point in a FID trace
pub struct ChemstationFidRecord {
    /// The time recorded at
    pub time: f64,
    /// The intensity recorded
    pub intensity: f64,
}

impl_record!(ChemstationFidRecord: time, intensity);

impl<'r> FromBuffer<'r> for Option<ChemstationFidRecord> {
    type State = &'r mut ChemstationFidState;

    fn get(rb: &'r mut ReadBuffer, state: Self::State) -> Result<Self, EtError> {
        if rb.len() < 2 {
            rb.refill()?;
        }
        if rb.is_empty() && rb.eof() {
            return Ok(None);
        } else if rb.len() == 1 && rb.eof() {
            return Err(EtError::new("FID record was incomplete"));
        }
        let time = state.cur_time;
        state.cur_time += state.time_step;

        let intensity: i16 = rb.extract(Endian::Big)?;
        if intensity == 32767 {
            state.cur_delta = 0.;
            let high_value: i32 = rb.extract(Endian::Big)?;
            let low_value: u16 = rb.extract(Endian::Big)?;
            state.cur_intensity = f64::from(high_value) * 65534. + f64::from(low_value);
        } else {
            state.cur_delta += f64::from(intensity);
            state.cur_intensity += state.cur_delta;
        }

        Ok(Some(ChemstationFidRecord {
            time,
            intensity: state.cur_intensity * state.metadata.mult_correction
                + state.metadata.offset_correction,
        }))
    }
}

#[derive(Debug)]
/// Internal state for the ChemstationMs parser
pub struct ChemstationMsState {
    n_scans_left: usize,
    n_mzs_left: usize,
    cur_time: f64,
    metadata: ChemstationMetadata,
}

impl<'r> StateMetadata<'r> for ChemstationMsState {
    fn metadata(&self) -> BTreeMap<String, Value> {
        (&self.metadata).into()
    }
}

impl<'r> FromBuffer<'r> for ChemstationMsState {
    type State = ();

    fn get(mut rb: &'r mut ReadBuffer, _state: Self::State) -> Result<Self, EtError> {
        let header = read_agilent_header(&mut rb, true)?;
        let metadata = get_metadata(&header)?;
        let n_scans = u32::out_of(&header[278..], Endian::Big)? as usize;

        Ok(ChemstationMsState {
            n_scans_left: n_scans,
            n_mzs_left: 0,
            cur_time: 0.,
            metadata,
        })
    }
}

#[derive(Clone, Copy, Debug)]
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

impl<'r> FromBuffer<'r> for Option<ChemstationMsRecord> {
    type State = &'r mut ChemstationMsState;

    fn get(rb: &'r mut ReadBuffer, state: Self::State) -> Result<Self, EtError> {
        if state.n_scans_left == 0 {
            return Ok(None);
        }

        // refill case
        if state.n_mzs_left == 0 {
            // handle the record header
            let raw_n_mzs_left: u16 = rb.extract(Endian::Big)?;
            if raw_n_mzs_left <= 14 {
                return Err("Invalid record header".into());
            }
            state.n_mzs_left = usize::from((raw_n_mzs_left - 13) / 2);
            state.cur_time = f64::from(rb.extract::<u32>(Endian::Big)?) / 60000.;
            let _ = rb.extract::<&[u8]>(12_usize)?;
        };

        // just read the mz/intensity
        let mz = f64::from(rb.extract::<u16>(Endian::Big)?) / 20.;
        let raw_intensity: u16 = rb.extract(Endian::Big)?;
        let intensity =
            f64::from(raw_intensity & 16383) * 8f64.powi(i32::from(raw_intensity) >> 14);
        if state.n_mzs_left == 1 {
            state.n_scans_left -= 1;
            // eat the footer
            let _ = rb.extract::<&[u8]>(10_usize)?;
        }
        state.n_mzs_left -= 1;

        Ok(Some(ChemstationMsRecord {
            time: state.cur_time,
            mz,
            intensity,
        }))
    }
}

#[derive(Debug)]
/// Internal state for the ChemstationMwd parser
pub struct ChemstationMwdState {
    n_wvs_left: usize,
    cur_time: f64,
    cur_intensity: f64,
    time_step: f64,
    metadata: ChemstationMetadata,
}

impl<'r> StateMetadata<'r> for ChemstationMwdState {
    fn metadata(&self) -> BTreeMap<String, Value> {
        (&self.metadata).into()
    }
}

impl<'r> FromBuffer<'r> for ChemstationMwdState {
    type State = ();

    fn get(mut rb: &'r mut ReadBuffer, _state: Self::State) -> Result<Self, EtError> {
        let header = read_agilent_header(&mut rb, false)?;
        let metadata = get_metadata(&header)?;

        Ok(ChemstationMwdState {
            n_wvs_left: 0,
            cur_time: metadata.start_time,
            cur_intensity: 0.,
            time_step: 0.2,
            metadata,
        })
    }
}

#[derive(Clone, Debug)]
/// A single point from an e.g. moving wavelength detector trace
pub struct ChemstationMwdRecord<'r> {
    /// The name of the signal that's being tracked
    pub signal_name: &'r str,
    /// The time recorded at
    pub time: f64,
    /// The intensity recorded
    pub intensity: f64,
}

impl<'r> RecordHeader for ChemstationMwdRecord<'r> {
    fn header() -> Vec<String> {
        vec![
            "time".to_string(),
            "mz".to_string(),
            "intensity".to_string(),
        ]
    }
}

impl<'r> From<ChemstationMwdRecord<'r>> for Vec<Value<'r>> {
    fn from(record: ChemstationMwdRecord<'r>) -> Self {
        // signal name is something like "MWD A, Sig=210,5 Ref=360,100"
        let mz = record
            .signal_name
            .splitn(2, "Sig=")
            .nth(1)
            .and_then(|last_part| {
                last_part
                    .splitn(2, ',')
                    .next()
                    .and_then(|sig_name| sig_name.parse::<f64>().ok())
            })
            .unwrap_or_else(|| 0.);
        vec![record.time.into(), mz.into(), record.intensity.into()]
    }
}

impl<'r> FromBuffer<'r> for Option<ChemstationMwdRecord<'r>> {
    type State = &'r mut ChemstationMwdState;

    fn get(rb: &'r mut ReadBuffer, state: Self::State) -> Result<Self, EtError> {
        if rb.is_empty() && rb.eof() {
            return Ok(None);
        }
        if state.n_wvs_left == 0 {
            // mask out the top nibble because it's always 0b0001 (i hope?)
            state.n_wvs_left = usize::from(rb.extract::<u16>(Endian::Big)?) & 0b111111111111;
            if state.n_wvs_left == 0 {
                // TODO: consume the rest of the file so this can't accidentally repeat?
                return Ok(None);
            }
        }

        let time = state.cur_time;
        state.cur_time += state.time_step;

        let intensity: i16 = rb.extract(Endian::Big)?;
        if intensity == -32768 {
            state.cur_intensity = f64::from(rb.extract::<i32>(Endian::Big)?);
        } else {
            state.cur_intensity += f64::from(intensity);
        }
        state.n_wvs_left -= 1;

        Ok(Some(ChemstationMwdRecord {
            signal_name: &state.metadata.signal_name,
            time,
            intensity: state.cur_intensity * state.metadata.mult_correction
                + state.metadata.offset_correction,
        }))
    }
}

#[derive(Clone, Copy, Debug)]
/// Internal state for the ChemstationUv parser
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

    fn get(mut rb: &'r mut ReadBuffer, _state: Self::State) -> Result<Self, EtError> {
        let header = read_agilent_header(&mut rb, false)?;
        let n_scans = u32::out_of(&header[278..], Endian::Big)? as usize;

        // TODO: get other metadata
        Ok(ChemstationUvState {
            n_scans_left: n_scans,
            n_wvs_left: 0,
            cur_time: 0.,
            cur_wv: 0.,
            cur_intensity: 0.,
            wv_step: 0.,
        })
    }
}

#[derive(Clone, Copy, Debug)]
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

impl<'r> FromBuffer<'r> for Option<ChemstationUvRecord> {
    type State = &'r mut ChemstationUvState;

    fn get(rb: &'r mut ReadBuffer, state: Self::State) -> Result<Self, EtError> {
        if state.n_scans_left == 0 {
            return Ok(None);
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

        Ok(Some(ChemstationUvRecord {
            time: state.cur_time,
            wavelength: state.cur_wv,
            intensity: state.cur_intensity / 2000.,
        }))
    }
}

impl_reader!(
    /// A reader for a Chemstation FID file
    ChemstationFidReader,
    ChemstationFidRecord,
    ChemstationFidState,
    ()
);

impl_reader!(
    /// A reader for a Chemstation MS file
    ChemstationMsReader,
    ChemstationMsRecord,
    ChemstationMsState,
    ()
);

impl_reader!(
    /// A reader for a Chemstation MWD file
    ChemstationMwdReader,
    ChemstationMwdRecord,
    ChemstationMwdState,
    ()
);

impl_reader!(
    /// A reader for a Chemstation UV file
    ChemstationUvReader,
    ChemstationUvRecord,
    ChemstationUvState,
    ()
);

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::ReadBuffer;

    #[test]
    fn test_chemstation_reader_fid() -> Result<(), EtError> {
        let rb = ReadBuffer::from_slice(include_bytes!("../../tests/data/test_fid.ch"));
        let mut reader = ChemstationFidReader::new(rb, ())?;
        let ChemstationFidRecord { time, intensity } = reader.next()?.unwrap();
        // TODO: try to confirm this time is correct
        assert!((time - 20184.8775).abs() < 0.0001);
        assert!((intensity - 17.500).abs() < 0.001);

        let mut n_mzs = 1;
        while let Some(_) = reader.next()? {
            n_mzs += 1;
        }
        assert_eq!(n_mzs, 2699);
        Ok(())
    }

    #[test]
    fn test_chemstation_reader_ms() -> Result<(), EtError> {
        let rb = ReadBuffer::from_slice(include_bytes!(
            "../../tests/data/carotenoid_extract.d/MSD1.MS"
        ));
        let mut reader = ChemstationMsReader::new(rb, ())?;
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
        while let Some(_) = reader.next()? {
            n_mzs += 1;
        }
        assert_eq!(n_mzs, 95471);
        Ok(())
    }

    #[test]
    fn test_chemstation_reader_mwd() -> Result<(), EtError> {
        let rb = ReadBuffer::from_slice(include_bytes!(
            "../../tests/data/chemstation_mwd.d/mwd1A.ch"
        ));
        let mut reader = ChemstationMwdReader::new(rb, ())?;
        let ChemstationMwdRecord {
            time,
            signal_name,
            intensity,
        } = reader.next()?.unwrap();
        assert!((time - -0.039667).abs() < 0.000001);
        assert_eq!(signal_name, "MWD A, Sig=210,5 Ref=360,100");
        assert!((intensity - -36.34977).abs() < 0.00001);

        let mut n_mzs = 1;
        while let Some(_) = reader.next()? {
            n_mzs += 1;
        }
        assert_eq!(n_mzs, 1801);
        Ok(())
    }

    #[test]
    fn test_chemstation_reader_uv() -> Result<(), EtError> {
        let rb = ReadBuffer::from_slice(include_bytes!(
            "../../tests/data/carotenoid_extract.d/dad1.uv"
        ));
        let mut reader = ChemstationUvReader::new(rb, ())?;
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

    #[test]
    fn test_chemstation_reader_bad_fuzzes() -> Result<(), EtError> {
        let test_data = b"\x012>\n\n\n\n\n\n>*\n\x86\n>\n\n\n\n\n\n\n\n\x14\n\n\n\n\n\n\n\n\xaf%\xa8\x00\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\n\n\n\n\n\n\n\n\n\n\n\n\n>>>\n*\n\n>>\n\xe3\x86\x86\n>>\n\n\n\n>\n\n\n\xaf%\x00\x00\x00\x00\x00\x00\x01\x04\n\n\n\n\n\n\n\n\n\n\n\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\n\n\n\n\n\n\n\n\n\n\n\n\n\n>>>\n*\n\n>>>\n\n\n\n>\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\n\n\n\n\n\n\n\n>\n\n\n\n>";
        let rb = ReadBuffer::from_slice(test_data);
        assert!(ChemstationMsReader::new(rb, ()).is_err());

        let test_data = b"\x012>\n\n\n\n\n\n>*\n\x86\n>\n\n>\n\xE3\x86\n>\n>\n\n>\n\xE3\x86&\n>@\x10\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\n\n\n\n\n\n\n\n\n\n\n\n\n\n\x02Y\n\n\n\n\xE3\x86\x86\n>\n\n>\n\n>\n*\n\n\n>\n\n>\n\n>\n\xE3\n\n\n\n\n\n\x14\n\n\n\n>\n\xC8>\n\x86\n>\n\n\n\n\n\n\n\n\n\n\n\n>\n\xE3\xCD\xCD\xCD\x00\x00\n\n\n\n\n\n>\n\n>\n\x00\n\x00\n\n\n\n\n\n\n>\n\n\n>\n\n\n\n\n\n\n\n>\n\n\n\n\n\n>\n\n\n\n\x00\x00\n\n\n\x00\n\n\n\n\n\n\n\n\xE3\x00\x00\n\n\n\n\n>\n\n\n>\n\n\n\n\n\n\n\n>\n\n\n\n\n\n>\n\n\n\n\x00\x00\n\n\n\x00\n\n\n\n\n\n\n\n\xE3\x00\x00\x00>\x0b\n\x01\x00>\n\n\n\x00>\n\n\x01\x00>\n\n\n\n\x00\x00\n\n\n\x00\n\n\n\n\n>\n\n>\n\n\n\n\n\n\n\n\n\n\n\n\x02Y\n\n\n\n\xE3\x86\n>>*\n\x86\xE3\x86\n>>*\n\x86\x00R>N\x02\xE3\n>\n>\xC6\n\n>\n\xE3\x00\x00\x00\x00\x00\x00\n\n\n\n\n>\n\xE3\xCD\n>\n\n>\n\xE3\n>@W\n\n+\n\n\n>\n\n>\n\xE3>*\n\x86*\n\x86\xE3\x86\n>>*\n\x86\xE3\x86\n>>*\n\x86\x00R>N\x02\xE3\n>\n>\xC6\n\n>\n\xE3\x00\x00\x00\n\n\n\n\n\n\n\n\n\n\n\n\n\x02Y\n\n\n\n\xE3\x86\x86\n>\n\n>\n\n>\n*\n\n\n>\n\n>\n\n\n\n\n\n\n\n\n\n\n\n\x02Y\n\n\n\n\xE3\x86\x86\n>\n\n>\x01\x00\x00\x00\x00\x00\x00\x01>\n\n>\n\n>\n\xE3\n\n\n\n\n\x01\x00\x00\x00\x00\x00\x00\x00\n\xE3\n>@W>N\x02\xE3\n>\n>\xC6\n\n>\n\xE3\x00\x00\x00";
        let rb = ReadBuffer::from_slice(test_data);
        assert!(ChemstationMsReader::new(rb, ()).is_err());

        let test_data = b"\x012>\n\n\n\n\n\n\n\n\n\n\n\n\n\n\x14\n\n\n\n\n\n\n\n\xAF%\xA8\x00\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\nVVVVV\n\n\xAF%\xA8\x00\xFE\x00\x00\x00\x00\x00\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x80\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x08\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x00\x00\x00\x00\x02\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x02\x00\n\n\n\n\n\n\n\n\n>";
        let rb = ReadBuffer::from_slice(test_data);
        assert!(ChemstationMsReader::new(rb, ()).is_err());

        Ok(())
    }
}
