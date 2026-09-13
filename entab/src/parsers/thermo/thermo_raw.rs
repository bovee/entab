use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use alloc::{format, vec};
use core::char::{decode_utf16, REPLACEMENT_CHARACTER};
use core::convert::TryFrom;

use crate::parsers::common::{EndOfFile, Skip};
use crate::parsers::{extract, Endian, FromSlice};
use crate::record::{StateMetadata, Value};
use crate::EtError;
use crate::{impl_reader, impl_record};

/// A UTF-16 string with a u32 header describing its length
#[derive(Debug, Default)]
pub struct PascalString16(String);

impl<'b: 's, 's> FromSlice<'b, 's> for PascalString16 {
    type State = ();

    fn parse(
        buffer: &[u8],
        _eof: bool,
        consumed: &mut usize,
        _state: &mut Self::State,
    ) -> Result<bool, EtError> {
        let length = usize::try_from(extract::<u32>(buffer, &mut 0, &mut Endian::Little)?)?;
        if buffer.len() < 4 * 2 * length * 2 {
            return Err(EtError::from("PascalString ended abruptly").incomplete());
        }
        *consumed += 4 + 2 * length;
        Ok(true)
    }

    fn get(&mut self, buffer: &'b [u8], _state: &'s Self::State) -> Result<(), EtError> {
        let iter = (4..buffer.len())
            .step_by(2)
            .map(|i| u16::from_le_bytes([buffer[i], buffer[i + 1]]));
        self.0 = decode_utf16(iter)
            .map(|r| r.unwrap_or(REPLACEMENT_CHARACTER))
            .collect::<String>();
        Ok(())
    }
}

/// The post-data trailer for a Thermo RAW file
#[derive(Clone, Copy, Debug, Default)]
pub struct ThermoRawTrailer {
    metadata_start: usize,
    coeffs_start: usize,
    n_scans: usize,
    min_mz: f64,
    max_mz: f64,
    min_time: f64,
    max_time: f64,
}

impl<'b: 's, 's> FromSlice<'b, 's> for ThermoRawTrailer {
    type State = u32; // just the version number

    fn parse(
        buffer: &[u8],
        _eof: bool,
        consumed: &mut usize,
        version: &mut Self::State,
    ) -> Result<bool, EtError> {
        if *version >= 64 && buffer.len() < 592 + 6980 {
            return Err(EtError::from("Trailer too short for version >= 64").incomplete());
        } else if *version >= 64 {
            *consumed += 592 + 6980;
        } else if *version >= 50 && buffer.len() < 592 + 6816 {
            return Err(EtError::from("Trailer too short for version >= 50").incomplete());
        } else if *version >= 50 {
            *consumed += 592 + 6816;
        } else {
            return Err("Version must be >= 50".into());
        }

        Ok(true)
    }

    fn get(&mut self, buffer: &'b [u8], version: &'s Self::State) -> Result<(), EtError> {
        // 592 bytes for misc information
        self.n_scans = usize::try_from(u32::extract(&buffer[12..16], &Endian::Little)?)?;
        // OR? these seem to be the same: u32::extract(&buffer[7376..7380], &Endian::Little)?
        self.min_mz = f64::extract(&buffer[56..64], &Endian::Little)?;
        self.max_mz = f64::extract(&buffer[64..72], &Endian::Little)?;
        self.min_time = f64::extract(&buffer[72..80], &Endian::Little)?;
        self.max_time = f64::extract(&buffer[80..88], &Endian::Little)?;
        self.coeffs_start = if *version >= 64 {
            usize::try_from(u64::extract(&buffer[7448..7456], &Endian::Little)?)?
        } else {
            usize::try_from(u32::extract(&buffer[7368..7372], &Endian::Little)?)?
        };
        self.metadata_start = if *version >= 64 {
            usize::try_from(u64::extract(&buffer[7408..7416], &Endian::Little)?)?
        } else {
            usize::try_from(u32::extract(&buffer[28..32], &Endian::Little)?)?
        };

        Ok(())
    }
}

/// Scan metadata
#[derive(Clone, Copy, Debug, Default)]
pub struct ThermoRawScanMetadata {
    time: f64,
    low_mz: f64,
    high_mz: f64,
}

impl<'b: 's, 's> FromSlice<'b, 's> for ThermoRawScanMetadata {
    type State = u32;

    fn parse(
        buffer: &[u8],
        _eof: bool,
        consumed: &mut usize,
        version: &mut Self::State,
    ) -> Result<bool, EtError> {
        let length = if *version >= 66 {
            88
        } else if *version >= 64 {
            80
        } else {
            72
        };
        if buffer.len() < length {
            return Err(EtError::from("Scan metadata incomplete").incomplete());
        }
        *consumed += length;
        Ok(true)
    }

    fn get(&mut self, buffer: &'b [u8], _version: &'s Self::State) -> Result<(), EtError> {
        self.time = f64::extract(&buffer[24..32], &Endian::Little)?;
        self.low_mz = f64::extract(&buffer[56..64], &Endian::Little)?;
        self.high_mz = f64::extract(&buffer[64..72], &Endian::Little)?;
        Ok(())
    }
}

/// Coefficients and other data about a scan
#[derive(Clone, Copy, Debug, Default)]
pub struct ThermoRawScanCoeffs {
    n_coeffs: u32,
    a: f64,
    b: f64,
    c: f64,
}

impl ThermoRawScanCoeffs {
    /// Convert a raw signal to a m/z value
    #[must_use]
    pub fn to_mz(&self, n: f64) -> f64 {
        match self.n_coeffs {
            0 => n,
            4 => self.a + self.b / n + self.c / n.powi(2),
            5 | 7 => self.a + self.b / n.powi(2) + self.c / n.powi(4),
            _ => unreachable!("Unparseable number of coefficients"),
        }
    }
}

impl<'b: 's, 's> FromSlice<'b, 's> for ThermoRawScanCoeffs {
    type State = (u32, usize);

    fn parse(
        buffer: &[u8],
        _eof: bool,
        consumed: &mut usize,
        (version, coeff_pos): &mut Self::State,
    ) -> Result<bool, EtError> {
        let mut record_len = if *version >= 66 {
            140
        } else if *version >= 63 {
            132
        } else if *version >= 62 {
            124
        } else if *version >= 57 {
            84
        } else {
            44
        };
        if buffer.len() < record_len {
            return Err(EtError::from("Coefficient data incomplete").incomplete());
        }

        let n_reactions =
            usize::try_from(u32::extract(&buffer[record_len - 4..], &Endian::Little)?)?;
        if *version >= 66 {
            record_len += n_reactions * 56;
        } else {
            record_len += n_reactions * 32;
        }
        record_len += 24;
        if buffer.len() < record_len {
            return Err(EtError::from("Coefficient reactions incomplete").incomplete());
        }

        *coeff_pos = record_len - 4;
        let n_coeffs = usize::try_from(u32::extract(&buffer[*coeff_pos..], &Endian::Little)?)?;
        record_len += n_coeffs * 8 + 8;
        if buffer.len() < record_len {
            return Err(EtError::from("Coefficients incomplete").incomplete());
        }

        if *version >= 66 {
            let extra = usize::try_from(u32::extract(&buffer[record_len - 8..], &Endian::Little)?)?;
            record_len += 4 + 8 * extra;
            if buffer.len() < record_len {
                return Err(EtError::from("Coefficients incomplete").incomplete());
            }
        }
        *consumed += record_len;
        Ok(true)
    }

    fn get(&mut self, buffer: &'b [u8], (_, coeff_pos): &'s Self::State) -> Result<(), EtError> {
        self.n_coeffs = u32::extract(&buffer[*coeff_pos..], &Endian::Little)?;
        if self.n_coeffs == 4 {
            self.a = f64::extract(&buffer[*coeff_pos + 12..], &Endian::Little)?;
            self.b = f64::extract(&buffer[*coeff_pos + 20..], &Endian::Little)?;
            self.c = f64::extract(&buffer[*coeff_pos + 28..], &Endian::Little)?;
        } else if self.n_coeffs == 5 || self.n_coeffs == 7 {
            self.a = f64::extract(&buffer[*coeff_pos + 20..], &Endian::Little)?;
            self.b = f64::extract(&buffer[*coeff_pos + 28..], &Endian::Little)?;
            self.c = f64::extract(&buffer[*coeff_pos + 36..], &Endian::Little)?;
        } else if self.n_coeffs != 0 {
            return Err("Unexpected number of coefficients".into());
        }
        Ok(())
    }
}

/// The state of a parser that handles Thermo RAW files
#[derive(Clone, Copy, Debug, Default)]
pub struct ThermoRawParams {
    version: u32,
    data_start: usize,
    trailer_start: usize,
    trailer: Option<ThermoRawTrailer>,
}

/// The state of a parser that handles Thermo RAW files
#[derive(Clone, Copy, Debug, Default)]
pub struct ThermoRawState {
    version: u32,
    metadata_pos: usize,
    coeffs_pos: usize,
    n_scans_left: usize,
    n_chunks_left: usize,
    n_points_left: usize,
    chunk_has_adjustment: bool,
    extra_bytes: usize,
    cur_time: f64,
    cur_freq: f64,
    base_freq: f64,
    freq_step: f64,
    cur_coeffs: ThermoRawScanCoeffs,
    cur_adjustment: f64,
}

impl ThermoRawState {
    /// Update the positions of the pointer in the metadata and coefficients sections
    ///
    /// # Errors
    /// If the amount consumed is larger than expected, an error will be returned.
    pub fn data_consumed(&mut self, con: usize) -> Result<(), EtError> {
        if self.metadata_pos < con {
            return Err("Data section extended into metadata section".into());
        } else if self.coeffs_pos < con {
            return Err("Data section extended into coefficients section".into());
        }
        self.metadata_pos -= con;
        self.coeffs_pos -= con;
        Ok(())
    }
}

impl StateMetadata for ThermoRawState {
    fn metadata(&self) -> BTreeMap<String, Value> {
        let mut map = BTreeMap::new();
        drop(map.insert("version".to_string(), self.version.into()));
        map
    }

    fn header(&self) -> Vec<&str> {
        vec!["time", "mz", "intensity"]
    }
}

impl<'b: 's, 's> FromSlice<'b, 's> for ThermoRawState {
    type State = ThermoRawParams;

    fn parse(
        buffer: &[u8],
        eof: bool,
        consumed: &mut usize,
        state: &mut Self::State,
    ) -> Result<bool, EtError> {
        // make sure the entire file is read in. unfortunately a few of the metadata fields needed
        // to parse the main data body are located near the end of the file (e.g. times,
        // transformation coefficients to convert raw signals into m/zs) so this would basically
        // happen anyhow. by doing it here, we prevent having to reparse because of an incomplete
        // elsewhere
        if !EndOfFile::parse(buffer, eof, &mut 0, &mut ())? {
            return Ok(false);
        }

        // now return to regular parsing
        if buffer.len() < 1420 {
            return Err(EtError::from("Header much too short").incomplete());
        }
        if &buffer[..2] != b"\x01\xA1" {
            return Err("Bad magic".into());
        }
        if state.version == 0 {
            // get the version number
            state.version = u32::extract(&buffer[36..40], &Endian::Little)?;
        }

        if state.data_start == 0 && state.trailer_start == 0 {
            // the next value (40..48) is a u64 with the run time (?) in "windows time"
            let con = &mut 1420;
            for _ in 1..=13 {
                let _ = PascalString16::parse(&buffer[*con..], eof, con, &mut ())?;
            }
            if state.version >= 50 {
                for _ in 14..=16 {
                    let _ = PascalString16::parse(&buffer[*con..], eof, con, &mut ())?;
                }
                let _ = extract::<u32>(buffer, con, &mut Endian::Little)?;
            }
            if state.version >= 60 {
                for _ in 17..=31 {
                    let _ = PascalString16::parse(&buffer[*con..], eof, con, &mut ())?;
                }
            }

            if state.version < 57 {
                // TODO: find some examples of these?
                return Err(format!(
                    "Old RAW files (version {}) are not supported yet",
                    state.version
                )
                .into());
            } else if state.version < 64 {
                let _: Skip = extract(buffer, con, &mut 52)?;
                state.data_start =
                    usize::try_from(extract::<u32>(buffer, con, &mut Endian::Little)?)?;
                let _: Skip = extract(buffer, con, &mut 16)?;
                state.trailer_start =
                    usize::try_from(extract::<u32>(buffer, con, &mut Endian::Little)?)?;
            } else {
                let _: Skip = extract(buffer, con, &mut 836)?;
                state.data_start =
                    usize::try_from(extract::<u64>(buffer, con, &mut Endian::Little)?)?;
                let _: Skip = extract(buffer, con, &mut 8)?;
                state.trailer_start =
                    usize::try_from(extract::<u64>(buffer, con, &mut Endian::Little)?)?;
            }
        }

        if state.trailer.is_none() {
            // load everything until the run_header_start; annoyingly, we need to parse the trailer
            // *after* all of the data to convert the "signal id" into a m/z for Orbtraps and other
            // instruments
            let _: Skip = extract(buffer, &mut 0, &mut state.trailer_start)?;
            let mut trailer_start = state.trailer_start;
            state.trailer = Some(extract::<ThermoRawTrailer>(
                buffer,
                &mut trailer_start,
                &mut state.version,
            )?);
        }

        *consumed += state.data_start;
        Ok(true)
    }

    fn get(&mut self, buffer: &'b [u8], state: &'s Self::State) -> Result<(), EtError> {
        self.version = u32::extract(&buffer[36..40], &Endian::Little)?;
        let trailer = state
            .trailer
            .ok_or_else(|| EtError::from("Trailer missing?"))?;
        self.metadata_pos = trailer.metadata_start - state.data_start;
        self.coeffs_pos = trailer.coeffs_start - state.data_start + 4;
        self.n_scans_left = trailer.n_scans;
        Ok(())
    }
}

/// A single data point from a Thermo RAW file
#[derive(Clone, Copy, Debug, Default)]
pub struct ThermoRawRecord {
    /// The time the reading was taken at
    pub time: f64,
    /// The mz value of the reading
    pub mz: f64,
    /// The intensity recorded
    pub intensity: f32,
}

impl_record!(ThermoRawRecord: time, mz, intensity);

impl<'b: 's, 's> FromSlice<'b, 's> for ThermoRawRecord {
    type State = ThermoRawState;

    fn parse(
        buffer: &[u8],
        _eof: bool,
        consumed: &mut usize,
        state: &mut Self::State,
    ) -> Result<bool, EtError> {
        let mut con = 0;
        if state.n_scans_left == 0 && state.n_chunks_left == 0 && state.n_points_left == 0 {
            return Ok(false);
        }
        let mut extra_bytes = state.extra_bytes;
        let mut n_scans_left = state.n_scans_left;
        let mut n_chunks_left = state.n_chunks_left;
        if state.n_chunks_left == 0 && state.n_points_left == 0 {
            let mut size_data = 0;
            while size_data == 0 {
                // skip the trailer from the last scan
                let _ = extract::<Skip>(buffer, &mut con, &mut extra_bytes)?;

                // read the extra metadata from the end of the file
                let scan_metadata: ThermoRawScanMetadata =
                    extract(buffer, &mut state.metadata_pos, &mut state.version)?;
                state.cur_time = scan_metadata.time;
                state.cur_coeffs = extract(buffer, &mut state.coeffs_pos, &mut (state.version, 0))?;

                // now read the record header itself
                let _ = extract::<Skip>(buffer, &mut con, &mut 4)?;
                size_data = extract::<u32>(buffer, &mut con, &mut Endian::Little)?;
                extra_bytes =
                    4 * usize::try_from(extract::<u32>(buffer, &mut con, &mut Endian::Little)?)?;
                // only the second bit `01000000` is ever set here?
                state.chunk_has_adjustment =
                    extract::<u32>(buffer, &mut con, &mut Endian::Little)? != 0;
                // three more sections we need to skip
                extra_bytes +=
                    4 * usize::try_from(extract::<u32>(buffer, &mut con, &mut Endian::Little)?)?;
                extra_bytes +=
                    4 * usize::try_from(extract::<u32>(buffer, &mut con, &mut Endian::Little)?)?;
                extra_bytes +=
                    4 * usize::try_from(extract::<u32>(buffer, &mut con, &mut Endian::Little)?)?;
                let _ = extract::<Skip>(buffer, &mut con, &mut 12)?;

                n_scans_left -= 1;
                if n_scans_left == 0 {
                    state.n_scans_left = n_scans_left;
                    return Ok(false);
                }
            }
            state.base_freq = extract(buffer, &mut con, &mut Endian::Little)?;
            state.freq_step = extract(buffer, &mut con, &mut Endian::Little)?;
            n_chunks_left =
                usize::try_from(extract::<u32>(buffer, &mut con, &mut Endian::Little)?)?;
            let _ = extract::<Skip>(buffer, &mut con, &mut 4)?;
        }
        if state.n_points_left == 0 {
            // read a chunk header
            let freq_offset = f64::from(extract::<u32>(buffer, &mut con, &mut Endian::Little)?);
            state.cur_freq = state.base_freq + state.freq_step * freq_offset - state.freq_step;
            state.n_points_left =
                usize::try_from(extract::<u32>(buffer, &mut con, &mut Endian::Little)?)?;
            if state.chunk_has_adjustment {
                state.cur_adjustment =
                    f64::from(extract::<f32>(buffer, &mut con, &mut Endian::Little)?);
            }
            n_chunks_left -= 1;
        }
        // include the point itself
        let _ = extract::<Skip>(buffer, &mut con, &mut 4)?;
        state.cur_freq += state.freq_step;

        state.n_scans_left = n_scans_left;
        state.n_chunks_left = n_chunks_left;
        state.n_points_left -= 1;
        state.extra_bytes = extra_bytes;
        state.data_consumed(con)?;
        *consumed += con;
        Ok(true)
    }

    fn get(&mut self, buffer: &'b [u8], state: &'s Self::State) -> Result<(), EtError> {
        self.time = state.cur_time;
        self.mz = state.cur_coeffs.to_mz(state.cur_freq) + state.cur_adjustment;
        self.intensity = f32::extract(&buffer[buffer.len() - 4..], &Endian::Little)?;
        Ok(())
    }
}

impl_reader!(
    ThermoRawReader,
    ThermoRawRecord,
    ThermoRawRecord,
    ThermoRawState,
    ThermoRawParams
);

// D648 - binary records (300 bytes long)
//
// 1464 - file path?

// for ThermoRawFileMS1MS2 -> start time = 0x25550C & 0x285A46
//

#[cfg(test)]
mod tests {
    use super::*;
    use crate::readers::RecordReader;

    #[test]
    fn test_thermo_raw() -> Result<(), EtError> {
        let rb: &[u8] = include_bytes!("../../../tests/data/small.RAW");
        let mut reader = ThermoRawReader::new(rb, None)?;
        let metadata = reader.metadata();
        assert_eq!(metadata["version"], 57.into());
        if let Some(ThermoRawRecord {
            time,
            mz,
            intensity,
        }) = reader.next()?
        {
            assert!((time - 0.004935).abs() < 0.000001);
            assert!((mz - 202.60682348271376).abs() < 0.000001);
            assert!((intensity - 1938.1174).abs() < 0.000001);
        } else {
            panic!("Thermo Raw reader returned bad record");
        }
        while reader.next()?.is_some() {}
        Ok(())
    }
}
