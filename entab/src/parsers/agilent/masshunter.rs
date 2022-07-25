use alloc::collections::BTreeMap;
use alloc::string::ToString;
use std::fs::File;
use std::path::Path;

use crate::buffer::ReadBuffer;
use crate::impl_record;
use crate::parsers::{Endian, FromSlice};
use crate::readers::{init_state, RecordReader};
use crate::record::{StateMetadata, Value};
use crate::EtError;

/// Store the current state of the `MasshunterDadReader`
#[derive(Copy, Clone, Debug, Default)]
pub struct MasshunterDadState {
    n_scans: u64,
    n_pts: u32,
    skip_data_bytes: usize,
    cur_time: f64,
    cur_wavelength: f64,
    wavelength_step: f64,
}

impl<'s> StateMetadata for MasshunterDadState {
    fn header(&self) -> Vec<&str> {
        vec!["time", "wavelength", "intensity"]
    }
}

impl<'b: 's, 's> FromSlice<'b, 's> for MasshunterDadState {
    type State = ();

    fn parse(
        buf: &[u8],
        _eof: bool,
        consumed: &mut usize,
        _state: &mut Self::State,
    ) -> Result<bool, EtError> {
        if buf.len() < 164 {
            return Err(EtError::from("Header is too short").incomplete());
        }
        *consumed += 164;
        Ok(true)
    }

    fn get(&mut self, buf: &'b [u8], _state: &'s Self::State) -> Result<(), EtError> {
        self.n_scans = u64::extract(&buf[80..88], &Endian::Little)?;
        self.n_pts = 1;
        self.skip_data_bytes = 68;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default)]
/// A dummy record so the header will update the current state
pub struct MasshunterDadHeaderRecord {}

impl<'b: 's, 's> FromSlice<'b, 's> for MasshunterDadHeaderRecord {
    type State = MasshunterDadState;

    /// The header file (.sd) is just a series of 80-byte chunks describing each scan in the run.
    ///  0 - u32,
    ///  4 - f64,  // time
    /// 12 - f64,
    /// 20 - f64,  // wavelength step
    /// 28 - u32,
    /// 32 - u64,  // pointer to start of scan in data file (plus 16 byte header)
    /// 40 - u32,
    /// 44 - u32,  // npts
    /// 48 - f64,  // wavelength start
    /// 56 - f64,
    /// 64 - f64,
    /// 72 - f64
    fn parse(
        buf: &[u8],
        _eof: bool,
        consumed: &mut usize,
        state: &mut Self::State,
    ) -> Result<bool, EtError> {
        state.n_pts -= 1;
        state.cur_wavelength += state.wavelength_step;
        if state.n_scans == 0 && state.n_pts == 0 {
            return Ok(true);
        } else if state.n_pts > 0 {
            // don't update if we're not starting a new record
            state.skip_data_bytes = 0;
            return Ok(true);
        }
        if buf.len() < 80 {
            // undo previous changes so we can re-enter above properly
            state.n_pts += 1;
            state.cur_wavelength -= state.wavelength_step;
            return Err(EtError::from("Record header is too short").incomplete());
        }
        state.skip_data_bytes += 16;
        state.cur_time = f64::extract(&buf[4..12], &Endian::Little)?;
        state.wavelength_step = f64::extract(&buf[20..28], &Endian::Little)?;
        state.n_pts = u32::extract(&buf[44..48], &Endian::Little)?;
        state.cur_wavelength = f64::extract(&buf[48..56], &Endian::Little)?;
        state.n_scans -= 1;
        *consumed += 80;
        Ok(true)
    }

    fn get(&mut self, _buf: &'b [u8], _state: &'s Self::State) -> Result<(), EtError> {
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default)]
/// The intensity at a single wavelength in a DAD run
pub struct MasshunterDadRecord {
    /// The time recorded at
    pub time: f64,
    /// The wavelength
    pub wavelength: f64,
    /// The intensity recorded
    pub intensity: f64,
}

impl_record!(MasshunterDadRecord: time, wavelength, intensity);

impl<'b: 's, 's> FromSlice<'b, 's> for MasshunterDadRecord {
    type State = MasshunterDadState;

    fn parse(
        buffer: &[u8],
        _eof: bool,
        consumed: &mut usize,
        state: &mut Self::State,
    ) -> Result<bool, EtError> {
        if state.n_scans == 0 && state.n_pts == 0 {
            return Ok(false);
        }
        if buffer.len() < state.skip_data_bytes + 8 {
            return Err(EtError::from("Data file is too short").incomplete());
        }
        *consumed += state.skip_data_bytes + 8;
        Ok(true)
    }

    fn get(&mut self, buf: &'b [u8], state: &'s Self::State) -> Result<(), EtError> {
        self.time = state.cur_time;
        self.wavelength = state.cur_wavelength;
        self.intensity = f64::extract(&buf[buf.len() - 8..], &Endian::Little)?;
        Ok(())
    }
}

/// Read Masshunter DAD files
#[derive(Debug)]
pub struct MasshunterDadReader<'r> {
    header_rb: ReadBuffer<'r>,
    data_rb: ReadBuffer<'r>,
    state: MasshunterDadState,
}

impl<'r> MasshunterDadReader<'r> {
    /// Create a new `MasshunterDadReader`
    ///
    /// # Errors
    /// If the file doesn't exist or can't be opened, an error will be returned.
    pub fn new<B>(data: B, params: Option<String>) -> Result<Self, EtError>
    where
        B: ::core::convert::TryInto<ReadBuffer<'r>>,
        EtError: From<<B as ::core::convert::TryInto<ReadBuffer<'r>>>::Error>,
    {
        let filename = params.ok_or_else(|| {
            EtError::new("Parser requires a filename; streams can not be parsed.")
        })?;
        let mut header_filename = Path::new(&filename).to_path_buf();
        let _ = header_filename.set_extension("sd");
        let header_file = File::open(header_filename)?;
        let (header_rb, state) = init_state::<_, File, _>(header_file, None)?;

        let data_rb = data.try_into()?;
        Ok(MasshunterDadReader {
            header_rb,
            data_rb,
            state,
        })
    }

    /// Return the next record
    ///
    /// # Errors
    /// If the next record can't be read, returns an error.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Result<Option<MasshunterDadRecord>, EtError> {
        if self
            .header_rb
            .next::<MasshunterDadHeaderRecord>(&mut self.state)?
            .is_none()
        {
            return Ok(None);
        }
        self.data_rb.next::<MasshunterDadRecord>(&mut self.state)
    }
}

impl<'r> RecordReader for MasshunterDadReader<'r> {
    /// The next record, expressed as a `Vec` of `Value`s.
    fn next_record(&mut self) -> Result<Option<::alloc::vec::Vec<Value>>, EtError> {
        Ok(self.next()?.map(core::convert::Into::into))
    }

    /// The headers for this Reader.
    fn headers(&self) -> ::alloc::vec::Vec<::alloc::string::String> {
        self.state
            .header()
            .iter()
            .map(|s| (*s).to_string())
            .collect()
    }

    /// The metadata for this Reader.
    fn metadata(&self) -> BTreeMap<String, Value> {
        self.state.metadata()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_dad_file() -> Result<(), EtError> {
        let mut filename = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let _ = filename.push("tests/data/masshunter_example/AcqData/DAD1.sp");

        let data: &[u8] = include_bytes!("../../../tests/data/masshunter_example/AcqData/DAD1.sp");
        let mut reader =
            MasshunterDadReader::new(data, Some(filename.to_str().unwrap().to_string()))?;
        assert_eq!(reader.headers(), ["time", "wavelength", "intensity"]);
        let _ = reader.metadata();
        let MasshunterDadRecord {
            time,
            wavelength,
            intensity,
        } = reader.next()?.unwrap();
        assert!((time - 0.0002083).abs() < 0.000001);
        assert!((wavelength - 250.).abs() < 0.00001);
        assert!((intensity - -1.03188).abs() < 0.00001);

        let mut n_mzs = 1;
        while reader.next()?.is_some() {
            n_mzs += 1;
        }
        assert_eq!(n_mzs, 240 * 276);
        Ok(())
    }
}
