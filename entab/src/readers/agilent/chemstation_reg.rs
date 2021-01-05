use alloc::collections::BTreeMap;
use core::marker::Copy;

use encoding::all::ISO_8859_1;
use encoding::{DecoderTrap, Encoding};

use crate::buffer::ReadBuffer;
use crate::parsers::{Endian, FromBuffer, FromSlice};
use crate::record::StateMetadata;
use crate::EtError;
use crate::{impl_reader, impl_record};
use crate::record::Value;

fn decode_iso_8859(raw: &[u8]) -> Result<String, EtError> {
    ISO_8859_1.decode(raw, DecoderTrap::Ignore).map_err(|e| e.as_ref().into())
}

/// State of the Chemstation REG parser
#[derive(Clone, Copy, Debug, Default)]
pub struct ChemstationRegState {}

impl<'r> StateMetadata<'r> for ChemstationRegState {}

impl<'r> FromBuffer<'r> for ChemstationRegState {
    type State = ();

    fn from_buffer(&mut self, rb: &'r mut ReadBuffer, _state: Self::State) -> Result<bool, EtError> {
        let header = rb.extract::<&[u8]>(45)?;

        println!("{:x?}", &header[20..30]);
        if header[25] != b'A' {
            return Err(EtError::new("Version of REG file is too new", &rb));
        }
        let n_sections = u16::out_of(&header[38..], Endian::Little)?;

        // TODO: parse multiple sections

        let n_records = rb.extract::<u32>(Endian::Little)? as usize;

        let mut records = Vec::with_capacity(n_records);
        for _ in 0..n_records {
            let _ = rb.extract::<u16>(Endian::Little)?;
            let record_type = rb.extract::<u16>(Endian::Little)?;
            let record_len = rb.extract::<u32>(Endian::Little)? as usize;
            let _ = rb.extract::<u32>(Endian::Little)?;
            let record_id = rb.extract::<u32>(Endian::Little)?;
            records.push((record_type, record_len, record_id))
        }

        let mut names: BTreeMap<u32, String> = BTreeMap::new();
        let mut metadata: BTreeMap<u32, Value> = BTreeMap::new();
        for (record_type, record_len, record_id) in records {
            let record_data = rb.extract::<&[u8]>(record_len)?;
            match record_type {
                // x-y table
                1281 | 1283 => { 
                   	// u16,u16,u8,u32,u32 (n_points),i16,u32,f64
                   	// H H B I I h I d
                   	
                   	// (then repeated twice, first x array and then y array)
                   	// u32 (units id),u32 (name id?),[12],i16,u32,f64 (multiplicative adjustment),f64,u64,u64,u8,[8]
                   	// I I 12s h I d d Q Q B 8s
					// FIXME
                },
                // key-value?
                1537 => {
                    // the matching data is in a 32770 record so we only get the name
                    let record_id = u32::out_of(&record_data[35..], Endian::Little)?;
                    let _ = names.insert(record_id, decode_iso_8859(&record_data[14..30].split(|c| *c == 0).next().unwrap_or(&record_data[14..30]))?);
                },
                // part of a linked list
                1538 => {
                    if record_data.len() != 39 {
                        return Err(EtError::new("Data type 1538 was an unexpected size", &rb));
                    }
                    let _ = names.insert(record_id, decode_iso_8859(&record_data[14..35])?);
                    let _ = metadata.insert(record_id, u32::out_of(&record_data[35..], Endian::Little)?.into());
                },
                // another part of a linked list with a table reference
                1539 => {
                    if record_data.len() != 39 {
                        return Err(EtError::new("Data type 1539 was an unexpected size", &rb));
                    }
                    let id = u32::out_of(&record_data[35..], Endian::Little)?;
                    let _ = names.insert(id, decode_iso_8859(&record_data[14..35])?);
                    // no data?
                },
                // table of values
                1793 => {
					let n_rows = u16::out_of(&record_data[4..], Endian::Little)?;
					let n_columns = u16::out_of(&record_data[16..], Endian::Little)?;
					if n_columns == 0 {
						continue;
					}
					// FIXME
                },
                // names (these have data elsewhere?)
                32769 | 32771 => {
                    let _ = names.insert(record_id, decode_iso_8859(&record_data[..record_len-1])?);
                },
                32774 => {
                    let _ = names.insert(record_id, decode_iso_8859(&record_data[2..record_len-1])?);
                },
                // flattened numeric array; contains the raw data for 1281/1283 records
                32770 => {
                    if record_data.len() < 4 {
                        return Err(EtError::new("Array was undersized", &rb));
                    }
                    let n_points = record_data.len() / 4 - 1;
                    let mut data: Vec<Value> = Vec::with_capacity(n_points);
                    for ix in 0..n_points {
                        data.push(u32::out_of(&record_data[4 * ix + 4..], Endian::Little)?.into());
                    }
                    let _ = metadata.insert(record_id, data.into());
                },
                _ => { },
            }
        }


        Ok(true)
    }
}

/// Record
#[derive(Clone, Copy, Debug, Default)]
pub struct ChemstationRegRecord {
    point: f64
}

impl<'r> FromBuffer<'r> for ChemstationRegRecord {
    type State = &'r mut ChemstationRegState;

    fn from_buffer(&mut self, _rb: &'r mut ReadBuffer, _state: Self::State) -> Result<bool, EtError> {
        Ok(false)
    }
}

impl_record!(ChemstationRegRecord: point);

impl_reader!(ChemstationRegReader, ChemstationRegRecord, ChemstationRegState, ());

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fcs_reader() -> Result<(), EtError> {
        let rb = ReadBuffer::from_slice(include_bytes!("../../../tests/data/chemstation_mwd.d/LCDIAG.REG"));
        let mut reader = ChemstationRegReader::new(rb, ())?;

        let mut n_recs = 1;
        while let Some(_) = reader.next()? {
            n_recs += 1;
        }
        assert_eq!(n_recs, 5);
        Ok(())
    }
}
