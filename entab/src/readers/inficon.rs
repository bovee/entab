use alloc::vec::Vec;
use alloc::{format, vec};
use core::marker::Copy;

use crate::buffer::ReadBuffer;
use crate::parsers::{Endian, FromBuffer};
use crate::record::StateMetadata;
use crate::EtError;
use crate::{impl_reader, impl_record};

/// The current state of the Inficon reader
#[derive(Clone, Debug, Default)]
pub struct InficonState {
    mz_segments: Vec<Vec<f64>>,
    data_end: u64,
    cur_time: f64,
    cur_segment: usize,
    mzs_left: usize,
}

impl<'r> StateMetadata<'r> for InficonState {}

impl<'r> FromBuffer<'r> for InficonState {
    type State = ();

    fn from_buffer(&mut self, rb: &'r mut ReadBuffer, _state: Self::State) -> Result<bool, EtError> {
        // probably not super robust, but it works? this appears at the end of
        // the "instrument collection steps" section and it appears to be
        // a constant distance before the "list of mzs" section
        if !rb.seek_pattern(b"\xFF\xFF\xFF\xFF\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\xF6\xFF\xFF\xFF\x00\x00\x00\x00")? {
            return Err(EtError::new("Could not find m/z header list"));
        }
        let _ = rb.extract::<&[u8]>(148)?;
        let n_segments = rb.extract::<u32>(Endian::Little)? as usize;
        if n_segments > 10000 {
            return Err(EtError::new("Inficon file has too many segments"));
        }
        // now read all of the collection segments
        let mut mz_segments = vec![Vec::new(); n_segments];
        for segment in mz_segments.iter_mut() {
            // first 4 bytes appear to be an name/identifier? not sure what
            // the rest is.
            let _ = rb.extract::<&[u8]>(96)?;
            let n_mzs = rb.extract::<u32>(Endian::Little)?;
            for _ in 0..n_mzs {
                let start_mz = rb.extract::<u32>(Endian::Little)?;
                let end_mz = rb.extract::<u32>(Endian::Little)?;
                if start_mz >= end_mz || end_mz >= 1e11 as u32 {
                    // only malformed data should hit this
                    return Err(EtError::new("m/z range is too big or invalid"));
                }
                // then dwell time (u32; microseconds) and three more u32s
                let _ = rb.extract::<&[u8]>(16)?;
                let i_type = rb.extract::<u32>(Endian::Little)?;
                let _ = rb.extract::<&[u8]>(4)?;
                if i_type == 0 {
                    // this is a SIM
                    segment.push(f64::from(start_mz) / 100.);
                } else {
                    // i_type = 1 appears to be "full scan mode"
                    let mut mz = start_mz;
                    while mz < end_mz + 1 {
                        segment.push(f64::from(mz) / 100.);
                        mz += 100;
                    }
                }
            }
        }
        self.mz_segments = mz_segments;
        if !rb.seek_pattern(b"\xFF\xFF\xFF\xFFHapsGPIR")? {
            return Err(EtError::new("Could not find start of scan data"));
        }
        // seek to right before the "HapsScan" section because the section
        // length is encoded in the four bytes before the header for that
        let _ = rb.extract::<&[u8]>(180)?;
        let data_length = u64::from(rb.extract::<u32>(Endian::Little)?);
        let _ = rb.extract::<&[u8]>(8)?;
        if rb.extract::<&[u8]>(8)? != b"HapsScan" {
            return Err(EtError::new("Data header was malformed"));
        }
        let _ = rb.extract::<&[u8]>(56)?;
        self.data_end = rb.get_byte_pos() + data_length;

        self.cur_time = 0.;
        self.cur_segment = 0;
        self.mzs_left = 0;
        Ok(true)
    }
}

/// A single record from an Inficon Hapsite file.
#[derive(Clone, Copy, Debug, Default)]
pub struct InficonRecord {
    time: f64,
    mz: f64,
    intensity: f64,
}

impl_record!(InficonRecord: time, mz, intensity);

impl<'r> FromBuffer<'r> for InficonRecord {
    type State = &'r mut InficonState;

    fn from_buffer(&mut self, rb: &'r mut ReadBuffer, state: Self::State) -> Result<bool, EtError> {
        if rb.get_byte_pos() >= state.data_end {
            return Ok(false);
        }
        if state.mzs_left == 0 {
            // the first u32 is the number of the record (i.e. from 1 to r_scans)
            let _ = rb.extract::<u32>(Endian::Little)?;
            state.cur_time = f64::from(rb.extract::<i32>(Endian::Little)?) / 60000.;
            // next value always seems to be 1
            let _ = rb.extract::<u16>(Endian::Little)?;
            let n_mzs = usize::from(rb.extract::<u16>(Endian::Little)?);
            // next value always seems to be 0xFFFF
            let _ = rb.extract::<u16>(Endian::Little)?;
            // the segment is only contained in the top nibble? the bottom is
            // F (e.g. values seem to be 0x0F, 0x1F, 0x2F...)
            state.cur_segment = usize::from(rb.extract::<u16>(Endian::Little)? >> 4);
            if state.cur_segment >= state.mz_segments.len() {
                return Err(EtError::new(format!(
                    "Invalid segment number ({}) specified",
                    state.cur_segment
                )));
            }
            if n_mzs != state.mz_segments[state.cur_segment].len() {
                return Err(EtError::new(format!(
                    "Number of intensities ({}) doesn't match number of mzs ({})",
                    n_mzs,
                    state.mz_segments[state.cur_segment].len()
                )));
            }
            state.mzs_left = n_mzs;
        }
        let intensity = f64::from(rb.extract::<f32>(Endian::Little)?);
        let cur_mz_segment = &state.mz_segments[state.cur_segment];
        let mz = cur_mz_segment[cur_mz_segment.len() - state.mzs_left];
        state.mzs_left -= 1;
        if state.mzs_left == 0 {
            rb.record_pos += 1;
        }
        self.time = state.cur_time;
        self.mz = mz;
        self.intensity = intensity;
        Ok(true)
    }
}

impl_reader!(
    /// A Reader for Inficon Hapsite data.
    ///
    /// This reader is currently untested on CI until we can find some test data
    /// that can be publicly distributed.
    InficonReader,
    InficonRecord,
    InficonState,
    ()
);

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn bad_inficon_fuzzes() -> Result<(), EtError> {
        let data = [
            4, 3, 2, 1, 83, 80, 65, 72, 66, 255, 255, 255, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 246, 255, 255, 255, 0, 0,
            0, 0, 14, 14, 14, 14, 14, 14, 14, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
            248, 10, 10, 10, 10, 35, 4, 0, 0, 0, 0, 0, 0, 10, 10, 10, 10, 10, 62, 10, 10, 26, 0, 0,
            0, 42, 42, 4, 0, 0, 0, 0, 0, 0, 10, 10, 10, 10, 10, 62, 10, 10, 10, 0, 0, 0, 0, 0, 0,
            0, 16, 42, 42, 42, 10, 62, 10, 10, 26, 0, 0, 0, 42, 42, 4, 0, 0, 0, 0, 0, 0, 10, 10,
            10, 10, 10, 62, 10, 10, 10, 0, 0, 0, 0, 0, 0, 0, 16, 42, 42, 42,
        ];
        let buffer = ReadBuffer::from_slice(&data);
        assert!(InficonReader::new(buffer, ()).is_err());

        let data = [
            4, 3, 2, 1, 83, 80, 65, 72, 4, 1, 10, 255, 255, 255, 0, 3, 197, 65, 77, 1, 62, 1, 0, 0,
            255, 255, 255, 255, 255, 255, 62, 10, 10, 10, 10, 62, 10, 10, 10, 8, 10, 62, 10, 10,
            62, 10, 10, 10, 9, 10, 62, 10, 10, 62, 10, 10, 62, 26, 10, 10, 10, 45, 10, 59, 9, 0,
            255, 255, 255, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 246, 255, 255, 255, 0, 0, 0, 0, 71, 71, 71, 71, 71, 38,
            200, 62, 10, 255, 255, 255, 255, 169, 77, 86, 139, 139, 116, 116, 116, 116, 116, 246,
            245, 245, 240, 255, 255, 241, 0, 0, 0, 0, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10,
            10, 10, 62, 10, 227, 205, 10, 10, 62, 10, 0, 62, 10, 10, 1, 0, 62, 10, 10, 34, 0, 0, 0,
            0, 0, 0, 0, 10, 10, 10, 10, 8, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10,
            10, 10, 245, 10, 10, 10, 10, 240, 10, 62, 10, 10, 10, 42, 10, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 168, 168, 168, 168, 168, 168, 168, 168, 168, 168, 168, 168, 168, 134, 134, 14,
            62, 10, 10, 62, 59, 42, 10, 10, 10, 62, 0, 13, 10, 10, 227, 10, 10, 62, 0, 13, 10, 10,
            227, 59, 10, 10, 0, 10, 10, 62, 41, 0, 13, 10, 10, 10, 227, 10, 10, 62, 0, 13, 10, 10,
            10, 62, 10, 10, 8, 10, 62, 10, 10, 10, 10, 10, 62, 10, 10, 10, 62, 10, 10, 10, 10, 62,
            10, 10, 10, 9, 10, 62, 10, 10, 255, 255, 255, 175, 255, 255, 255, 255, 255, 255, 255,
            255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
            255, 255, 255, 10, 10, 10, 9, 10, 62, 45, 10, 59, 9, 0,
        ];
        let buffer = ReadBuffer::from_slice(&data);
        assert!(InficonReader::new(buffer, ()).is_err());

        let data = [
            4, 3, 2, 1, 83, 80, 65, 72, 66, 65, 77, 1, 62, 1, 230, 255, 255, 251, 254, 254, 254,
            254, 168, 168, 168, 168, 168, 168, 168, 168, 168, 168, 168, 0, 10, 62, 10, 59, 10, 10,
            10, 10, 10, 10, 10, 10, 10, 10, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255,
            255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 246, 255, 255, 255, 0, 0, 0, 0, 10, 10, 102, 13, 10, 35, 24, 10, 62, 13,
            10, 13, 227, 5, 62, 10, 227, 134, 134, 10, 62, 10, 10, 62, 42, 10, 10, 10, 62, 0, 13,
            10, 10, 227, 10, 10, 62, 0, 13, 10, 10, 227, 59, 10, 10, 250, 255, 10, 62, 41, 0, 13,
            10, 10, 227, 43, 10, 10, 10, 10, 10, 10, 47, 59, 10, 10, 62, 0, 13, 10, 10, 227, 10,
            10, 227, 59, 10, 10, 0, 10, 10, 10, 10, 26, 10, 10, 41, 0, 13, 10, 10, 227, 59, 10, 10,
            10, 10, 10, 14, 10, 255, 255, 255, 255, 176, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 175, 255, 255, 255,
            255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
            255, 255, 255, 255, 245, 240, 255, 255, 255, 255, 255, 169, 77, 86, 139, 139, 116, 35,
            116, 116, 116, 246, 245, 245, 240, 250, 255, 10, 62, 41, 0, 13, 10, 10, 227, 43, 10,
            10, 10, 10, 10, 10, 47, 59, 10, 10, 4, 3, 2, 1, 83, 80, 181, 181, 181, 181, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255,
            255, 255, 255, 255, 58, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
            255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 122, 255, 255, 255,
            255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 246, 255, 255, 255, 0, 0, 0, 0, 59, 10, 10, 10, 10, 10, 14, 10, 255, 10,
            10, 10, 10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 116, 116, 246, 245, 245, 240,
        ];
        let buffer = ReadBuffer::from_slice(&data);
        assert!(InficonReader::new(buffer, ()).is_err());

        Ok(())
    }
}
