use alloc::boxed::Box;

use crate::buffer::Endian;
use crate::buffer::ReadBuffer;
use crate::readers::{ReaderBuilder, RecordReader};
use crate::record::Record;
use crate::EtError;

pub struct ChemstationMsReaderBuilder;

impl Default for ChemstationMsReaderBuilder {
    fn default() -> Self {
        ChemstationMsReaderBuilder
    }
}

impl ReaderBuilder for ChemstationMsReaderBuilder {
    fn to_reader<'r>(&self, mut rb: ReadBuffer<'r>) -> Result<Box<dyn RecordReader + 'r>, EtError> {
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

        Ok(Box::new(ChemstationMsReader {
            rb,
            n_scans_left: n_scans,
            n_mzs_left: 0,
            cur_time: 0.,
        }))
    }
}

pub struct ChemstationMsReader<'r> {
    rb: ReadBuffer<'r>,
    n_scans_left: usize,
    n_mzs_left: usize,
    cur_time: f64,
}

impl<'r> RecordReader for ChemstationMsReader<'r> {
    fn next(&mut self) -> Result<Option<Record>, EtError> {
        if self.n_scans_left == 0 {
            return Ok(None);
        }

        // refill case
        if self.n_mzs_left == 0 {
            // handle the record header
            let raw_n_mzs_left: u16 = self.rb.extract(Endian::Big)?;
            if raw_n_mzs_left <= 14 {
                return Err(EtError::new("Invalid record header").fill_pos(&self.rb));
            }
            self.n_mzs_left = usize::from((raw_n_mzs_left - 13) / 2);
            self.cur_time = f64::from(self.rb.extract::<u32>(Endian::Big)?) / 60000.;
            self.rb.extract(12_usize)?;
        };

        // just read the mz/intensity
        let mz = f64::from(self.rb.extract::<u16>(Endian::Big)?) / 20.;
        let raw_intensity: u16 = self.rb.extract(Endian::Big)?;
        let intensity = f64::from(raw_intensity & 16383) * 8f64.powi(i32::from(raw_intensity) >> 14);
        if self.n_mzs_left == 1 {
            self.n_scans_left -= 1;
            // eat the footer and bump the record number
            self.rb.extract(10_usize)?;
            self.rb.consume(0);
        }
        self.n_mzs_left -= 1;

        Ok(Some(Record::Mz {
            time: self.cur_time,
            mz,
            intensity,
        }))
    }
}

pub struct ChemstationFidReaderBuilder;

impl Default for ChemstationFidReaderBuilder {
    fn default() -> Self {
        ChemstationFidReaderBuilder
    }
}

impl ReaderBuilder for ChemstationFidReaderBuilder {
    fn to_reader<'r>(&self, mut rb: ReadBuffer<'r>) -> Result<Box<dyn RecordReader + 'r>, EtError> {
        rb.extract(282_usize)?;
        let time = f64::from(rb.extract::<u32>(Endian::Big)?) / 60000.;
        // next value (0x11E..) is the end time
        rb.extract(738_usize)?;

        Ok(Box::new(ChemstationFidReader {
            rb,
            time,
            intensity: 0,
        }))
    }
}

pub struct ChemstationFidReader<'r> {
    rb: ReadBuffer<'r>,
    time: f64,
    intensity: i64,
}

impl<'r> RecordReader for ChemstationFidReader<'r> {
    fn next(&mut self) -> Result<Option<Record>, EtError> {
        if self.rb.len() < 4 && self.rb.eof() {
            return Ok(None);
        }

        let time = self.time;
        // TODO: 0.2 / 60.0 should be obtained from the file???
        self.time += 0.2 / 60.;

        let intensity: i32 = self.rb.extract(Endian::Big)?;
        if intensity == 32767 {
            self.rb.reserve(6)?;
            let high_value: i32 = self.rb.extract(Endian::Big)?;
            let low_value: u16 = self.rb.extract(Endian::Big)?;
            self.intensity = i64::from(high_value) * 65534 + i64::from(low_value);
        } else {
            self.intensity += i64::from(intensity);
        }
        self.rb.consume(0);

        Ok(Some(Record::Mz {
            time,
            mz: 0.,
            intensity: self.intensity as f64,
        }))
    }
}

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
