use alloc::boxed::Box;

use crate::buffer::Endian;
use crate::buffer::ReadBuffer;
use crate::readers::{ReaderBuilder, RecordReader};
use crate::record::Record;
use crate::EtError;

pub struct ChemstationMsState {
    n_scans_left: usize,
    n_mzs_left: usize,
    cur_time: f64,
}

use crate::buffer::FromBuffer;

impl<'r> FromBuffer<'r> for ChemstationMsState {
    type State = ();

    fn get(rb: &'r mut ReadBuffer, _amt: Self::State) -> Result<Self, EtError> {
        rb.reserve(266)?;
        let n_scans_pos = if &rb[5..7] == b"GC" { 322 } else { 280 };
        rb.partial_consume(266);
        let raw_records_start = usize::from(rb.extract::<u16>(Endian::Big)?);
        if raw_records_start <= 142 {
            return Err("Invalid start position in header".into());
        }
        let records_start = 2 * raw_records_start - 2;
        rb.reserve(records_start - 268)?;
        rb.partial_consume(n_scans_pos - 268);
        if records_start < n_scans_pos {
            return Err(EtError::new("File ended abruptly"));
        }
        let n_scans = usize::from(rb.extract::<u16>(Endian::Big)?);
        rb.partial_consume(records_start - n_scans_pos - 2);

        Ok(ChemstationMsState {
            n_scans_left: n_scans,
            n_mzs_left: 0,
            cur_time: 0.,
        })
    }
}

pub struct ChemstationMs {
    time: f64,
    mz: f64,
    intensity: f64,
}

impl<'r> FromBuffer<'r> for Option<ChemstationMs> {
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
            rb.extract(12_usize)?;
        };

        // just read the mz/intensity
        let mz = f64::from(rb.extract::<u16>(Endian::Big)?) / 20.;
        let raw_intensity: u16 = rb.extract(Endian::Big)?;
        let intensity = f64::from(raw_intensity & 16383) * 8f64.powi(i32::from(raw_intensity) >> 14);
        if state.n_mzs_left == 1 {
            state.n_scans_left -= 1;
            // eat the footer and bump the record number
            rb.extract(10_usize)?;
            rb.consume(0);
        }
        state.n_mzs_left -= 1;

        Ok(Some(ChemstationMs {
            time: state.cur_time,
            mz,
            intensity,
        }))
    }
}

pub struct ChemstationMsReaderBuilder;

impl Default for ChemstationMsReaderBuilder {
    fn default() -> Self {
        ChemstationMsReaderBuilder
    }
}

impl ReaderBuilder for ChemstationMsReaderBuilder {
    fn to_reader<'r>(&self, mut rb: ReadBuffer<'r>) -> Result<Box<dyn RecordReader + 'r>, EtError> {
        let state = rb.extract(())?;
        Ok(Box::new(ChemstationMsReader {
            rb,
            state,
        }))
    }
}

pub struct ChemstationMsReader<'r> {
    rb: ReadBuffer<'r>,
    state: ChemstationMsState,
}

impl<'r> RecordReader for ChemstationMsReader<'r> {
    fn next(&mut self) -> Result<Option<Record>, EtError> {
        let record: Option<ChemstationMs> = self.rb.extract(&mut self.state)?;
        Ok(record.map(|ChemstationMs { time, mz, intensity }| {
            Record::Mz {
                time,
                mz,
                intensity,
            }
        }))
    }
}

pub struct ChemstationFidState {
    time: f64,
    intensity: f64,
}

impl<'r> FromBuffer<'r> for ChemstationFidState {
    type State = ();

    fn get(rb: &'r mut ReadBuffer, _amt: Self::State) -> Result<Self, EtError> {
        rb.extract(282_usize)?;
        let time = f64::from(rb.extract::<u32>(Endian::Big)?) / 60000.;
        // next value (0x11E..) is the end time
        rb.extract(738_usize)?;

        Ok(ChemstationFidState {
            time,
            intensity: 0.,
        })
    }
}

pub struct ChemstationFid {
    time: f64,
    intensity: f64,
}

impl<'r> FromBuffer<'r> for Option<ChemstationFid> {
    type State = &'r mut ChemstationFidState;

    fn get(rb: &'r mut ReadBuffer, state: Self::State) -> Result<Self, EtError> {
        if rb.len() < 4 && rb.eof() {
            return Ok(None);
        }

        let time = state.time;
        // TODO: 0.2 / 60.0 should be obtained from the file???
        state.time += 0.2 / 60.;

        let intensity: i32 = rb.extract(Endian::Big)?;
        if intensity == 32767 {
            rb.reserve(6)?;
            let high_value: i32 = rb.extract(Endian::Big)?;
            let low_value: u16 = rb.extract(Endian::Big)?;
            state.intensity = f64::from(high_value) * 65534. + f64::from(low_value);
        } else {
            state.intensity += f64::from(intensity);
        }
        rb.consume(0);

        Ok(Some(ChemstationFid {
            time,
            intensity: state.intensity,
        }))
    }
}


// scratch with offsets for info in different files

// FID - 02 38 31 00
//  * 264 - header_chunks // 2 + 1
//  * 282 - x min? (i32?)
//  * 286 - x max? (i32?)


// MWD - 02 33 30 00
//  - 24 - Sample Name
//  - 148 - Operator Name
//  - 178 - Run Date
//  - 228 - Method Name
//  * 264 - header_chunks // 2 + 1
//  * 282 - x min? (i32?)
//  * 286 - x max? (i32?)
//  - 580 - Units
//  - 596 - Channel Info
//   1024 - data start?

// LC - 03 31 33 31
//  * 264 - header_chunks // 2 + 1
//  - 858 - Sample Name
//  - 1880 - Operator Name
//  - 2391 - Run Date
//  - 2574 - Method Name
//  - 3093 - Units
//   4096 - data start?

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::ReadBuffer;

    #[cfg(feature = "std")]
    #[test]
    fn test_chemstation_reader() -> Result<(), EtError> {
        use std::fs::File;

        let f = File::open("tests/data/carotenoid_extract.d/MSD1.MS")?;
        let rb = ReadBuffer::new(Box::new(&f))?;
        let builder = ChemstationMsReaderBuilder::default();
        let mut reader = builder.to_reader(rb)?;
        if let Record::Mz {
            time,
            mz,
            intensity,
        } = reader.next()?.unwrap()
        {
            assert!((time - 0.079166).abs() < 0.000001);
            assert!((mz - 915.7).abs() < 0.000001);
            assert_eq!(intensity, 112.);
        } else {
            panic!("Chemstation reader returned non-Mz record");
        }
        if let Record::Mz {
            time,
            mz,
            intensity,
        } = reader.next()?.unwrap()
        {
            assert!((time - 0.079166).abs() < 0.000001);
            assert!((mz - 865.4).abs() < 0.000001);
            assert_eq!(intensity, 184.);
        } else {
            panic!("Chemstation reader returned non-Mz record");
        }
        let mut n_mzs = 2;
        while let Some(_) = reader.next()? {
            n_mzs += 1;
        }
        assert_eq!(n_mzs, 95471);
        Ok(())
    }

    #[test]
    fn test_chemstation_reader_bad_fuzzes() -> Result<(), EtError> {
        let test_data = b"\x012>\n\n\n\n\n\n>*\n\x86\n>\n\n\n\n\n\n\n\n\x14\n\n\n\n\n\n\n\n\xaf%\xa8\x00\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\n\n\n\n\n\n\n\n\n\n\n\n\n>>>\n*\n\n>>\n\xe3\x86\x86\n>>\n\n\n\n>\n\n\n\xaf%\x00\x00\x00\x00\x00\x00\x01\x04\n\n\n\n\n\n\n\n\n\n\n\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\n\n\n\n\n\n\n\n\n\n\n\n\n\n>>>\n*\n\n>>>\n\n\n\n>\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\n\n\n\n\n\n\n\n>\n\n\n\n>";
        let rb = ReadBuffer::from_slice(test_data);
        let builder = ChemstationMsReaderBuilder::default();
        assert!(builder.to_reader(rb).is_err());

        let test_data = b"\x012>\n\n\n\n\n\n>*\n\x86\n>\n\n>\n\xE3\x86\n>\n>\n\n>\n\xE3\x86&\n>@\x10\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF\n\n\n\n\n\n\n\n\n\n\n\n\n\n\x02Y\n\n\n\n\xE3\x86\x86\n>\n\n>\n\n>\n*\n\n\n>\n\n>\n\n>\n\xE3\n\n\n\n\n\n\x14\n\n\n\n>\n\xC8>\n\x86\n>\n\n\n\n\n\n\n\n\n\n\n\n>\n\xE3\xCD\xCD\xCD\x00\x00\n\n\n\n\n\n>\n\n>\n\x00\n\x00\n\n\n\n\n\n\n>\n\n\n>\n\n\n\n\n\n\n\n>\n\n\n\n\n\n>\n\n\n\n\x00\x00\n\n\n\x00\n\n\n\n\n\n\n\n\xE3\x00\x00\n\n\n\n\n>\n\n\n>\n\n\n\n\n\n\n\n>\n\n\n\n\n\n>\n\n\n\n\x00\x00\n\n\n\x00\n\n\n\n\n\n\n\n\xE3\x00\x00\x00>\x0b\n\x01\x00>\n\n\n\x00>\n\n\x01\x00>\n\n\n\n\x00\x00\n\n\n\x00\n\n\n\n\n>\n\n>\n\n\n\n\n\n\n\n\n\n\n\n\x02Y\n\n\n\n\xE3\x86\n>>*\n\x86\xE3\x86\n>>*\n\x86\x00R>N\x02\xE3\n>\n>\xC6\n\n>\n\xE3\x00\x00\x00\x00\x00\x00\n\n\n\n\n>\n\xE3\xCD\n>\n\n>\n\xE3\n>@W\n\n+\n\n\n>\n\n>\n\xE3>*\n\x86*\n\x86\xE3\x86\n>>*\n\x86\xE3\x86\n>>*\n\x86\x00R>N\x02\xE3\n>\n>\xC6\n\n>\n\xE3\x00\x00\x00\n\n\n\n\n\n\n\n\n\n\n\n\n\x02Y\n\n\n\n\xE3\x86\x86\n>\n\n>\n\n>\n*\n\n\n>\n\n>\n\n\n\n\n\n\n\n\n\n\n\n\x02Y\n\n\n\n\xE3\x86\x86\n>\n\n>\x01\x00\x00\x00\x00\x00\x00\x01>\n\n>\n\n>\n\xE3\n\n\n\n\n\x01\x00\x00\x00\x00\x00\x00\x00\n\xE3\n>@W>N\x02\xE3\n>\n>\xC6\n\n>\n\xE3\x00\x00\x00";
        let rb = ReadBuffer::from_slice(test_data);
        let builder = ChemstationMsReaderBuilder::default();
        let mut reader = builder.to_reader(rb)?;
        assert!(reader.next().is_err());

        let test_data = b"\x012>\n\n\n\n\n\n\n\n\n\n\n\n\n\n\x14\n\n\n\n\n\n\n\n\xAF%\xA8\x00\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\nVVVVV\n\n\xAF%\xA8\x00\xFE\x00\x00\x00\x00\x00\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x80\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x08\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x81\x00\x00\x00\x00\x02\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x02\x00\n\n\n\n\n\n\n\n\n>";
        let rb = ReadBuffer::from_slice(test_data);
        let builder = ChemstationMsReaderBuilder::default();
        assert!(builder.to_reader(rb).is_err());

        Ok(())
    }
}
