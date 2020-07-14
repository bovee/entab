use alloc::boxed::Box;

use byteorder::{BigEndian, ByteOrder};

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
        rb.reserve(268)?;
        let raw_records_start = usize::from(BigEndian::read_u16(&rb[266..268]));
        if raw_records_start <= 142 {
            return Err("Invalid start position in header".into());
        }
        let records_start = 2 * raw_records_start - 2;
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
    fn next(&mut self) -> Result<Option<Record>, EtError> {
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
            let raw_n_mzs_left = BigEndian::read_u16(&buf[..2]);
            if raw_n_mzs_left <= 14 {
                return Err(EtError::new("Invalid record header").fill_pos(&self.rb));
            }
            self.n_mzs_left = usize::from((raw_n_mzs_left - 13) / 2);
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

        Ok(Some(Record::Mz {
            time: self.cur_time,
            mz,
            intensity,
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
            assert_eq!(intensity, 112);
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
            assert_eq!(intensity, 184);
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
