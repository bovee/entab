use alloc::boxed::Box;
use alloc::format;
use alloc::vec;
use alloc::vec::Vec;

use byteorder::{BigEndian, ByteOrder};
use serde::Serialize;

use crate::buffer::ReadBuffer;
use crate::record::{BindT, ReaderBuilder, Record, RecordReader};
use crate::EtError;

#[derive(Debug, Serialize)]
pub struct MzRecord {
    time: f64,
    mz: f64,
    intensity: u64,
}

impl<'s> Record for MzRecord {
    fn size(&self) -> usize {
        3
    }

    fn write_field<W>(&self, index: usize, mut write: W) -> Result<(), EtError>
    where
        W: FnMut(&[u8]) -> Result<(), EtError>,
    {
        match index {
            0 => write(format!("{:02}", self.time).as_bytes())?,
            1 => write(format!("{:02}", self.mz).as_bytes())?,
            2 => write(format!("{:02}", self.intensity).as_bytes())?,
            _ => panic!("FASTA field index out of range"),
        };
        Ok(())
    }
}

pub struct MzRecordT;
impl<'b> BindT<'b> for MzRecordT {
    type Assoc = MzRecord;
}

pub struct ChemstationMsReaderBuilder;

impl Default for ChemstationMsReaderBuilder {
    fn default() -> Self {
        ChemstationMsReaderBuilder
    }
}

impl ReaderBuilder for ChemstationMsReaderBuilder {
    type Item = MzRecordT;

    fn to_reader<'r>(
        &self,
        mut rb: ReadBuffer<'r>,
    ) -> Result<Box<dyn RecordReader<Item = Self::Item> + 'r>, EtError> {
        rb.reserve(268)?;
        let records_start = 2 * usize::from(BigEndian::read_u16(&rb[266..268])) - 2;
        rb.reserve(records_start)?;

        let n_scans = {
            let header = rb.partial_consume(records_start);
            let n_scans_pos = if &header[5..7] == b"GC" { 324 } else { 282 };
            if records_start < n_scans_pos {
                return Err(EtError::new("File ended abruptly"));
            }
            usize::from(BigEndian::read_u16(&header[n_scans_pos - 2..n_scans_pos]))
        };

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
    type Item = MzRecordT;

    fn headers(&self) -> Vec<&str> {
        vec!["time", "mz", "intensity"]
    }

    fn next(&mut self) -> Result<Option<MzRecord>, EtError> {
        if self.n_scans_left == 0 {
            return Ok(None);
        }

        let read_amount = match self.n_mzs_left {
            0 => 22,
            1 => 14,
            _ => 4,
        };
        self.rb.reserve(read_amount)?;

        // refill case
        let rec = if self.n_mzs_left == 0 {
            let buf = self.rb.partial_consume(read_amount);
            // handle the record header
            self.n_mzs_left = usize::from((BigEndian::read_u16(&buf[..2]) - 13) / 2);
            self.cur_time = f64::from(BigEndian::read_u32(&buf[2..6])) / 60000.;
            &buf[18..]
        } else if self.n_mzs_left == 1 {
            // handle the record footer too
            self.n_scans_left -= 1;
            &self.rb.consume(read_amount)[..4]
        } else {
            // just read the mz/intensity
            self.rb.partial_consume(read_amount)
        };

        let mz = f64::from(BigEndian::read_u16(&rec[..2])) / 20.;
        let raw_intensity = BigEndian::read_u16(&rec[2..]);
        let intensity = u64::from(raw_intensity & 16383) * 8u64.pow(u32::from(raw_intensity) >> 14);
        self.n_mzs_left -= 1;

        Ok(Some(MzRecord {
            time: self.cur_time,
            mz,
            intensity,
        }))
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "std")]
    use super::*;
    #[cfg(feature = "std")]
    use crate::buffer::ReadBuffer;

    #[cfg(feature = "std")]
    #[test]
    fn test_chemstation_reader() -> Result<(), EtError> {
        use std::fs::File;

        let f = File::open("tests/data/carotenoid_extract.d/MSD1.MS")?;
        let rb = ReadBuffer::new(Box::new(&f))?;
        let builder = ChemstationMsReaderBuilder::default();
        let mut reader = builder.to_reader(rb)?;
        let rec = reader.next()?.unwrap();
        assert!((rec.time - 0.079166).abs() < 0.000001);
        assert!((rec.mz - 915.7).abs() < 0.000001);
        assert_eq!(rec.intensity, 112);
        let rec = reader.next()?.unwrap();
        assert!((rec.time - 0.079166).abs() < 0.000001);
        assert!((rec.mz - 865.4).abs() < 0.000001);
        assert_eq!(rec.intensity, 184);
        let mut n_mzs = 2;
        while let Some(_) = reader.next()? {
            n_mzs += 1;
        }
        assert_eq!(n_mzs, 95471);
        Ok(())
    }

    fn test_chemstation_reader_bad_fuzzes() -> Result<(), EtError> {
        use std::fs::File;

        let test_data = &[1, 50, 62, 10, 10, 10, 10, 10, 10, 62, 42, 10, 134, 10, 62, 10, 10, 10, 10, 10, 10, 10, 10, 20, 10, 10, 10, 10, 10, 10, 10, 10, 175, 37, 168, 0, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 62, 62, 62, 10, 42, 10, 10, 62, 62, 10, 227, 134, 134, 10, 62, 62, 10, 10, 10, 10, 62, 10, 10, 10, 175, 37, 0, 0, 0, 0, 0, 0, 1, 4, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 10, 62, 62, 62, 10, 42, 10, 10, 62, 62, 62, 10, 10, 10, 10, 62, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 10, 10, 10, 10, 10, 10, 10, 10, 62, 10, 10, 10, 10, 62];

        // let test_data = b"\x012>\n\n\n\n\n\n>*\n\x86\n>\n\n\n\n\n\n\n\n\x14\n\n\n\n\n\n\n\n\xaf%\xa8\x00\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\n\n\n\n\n\n\n\n\n\n\n\n\n>>>\n*\n\n>>\n\xe3\x86\x86\n>>\n\n\n\n>\n\n\n\xaf%\x00\x00\x00\x00\x00\x00\x01\x04\n\n\n\n\n\n\n\n\n\n\n\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\n\n\n\n\n\n\n\n\n\n\n\n\n\n>>>\n*\n\n>>>\n\n\n\n>\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\n\n\n\n\n\n\n\n>\n\n\n\n>";
        let rb = ReadBuffer::from_slice(test_data);
        let builder = ChemstationMsReaderBuilder::default();
        let mut reader = builder.to_reader(rb)?;
        Ok(())
    }
}
