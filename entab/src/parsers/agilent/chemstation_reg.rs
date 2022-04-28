use alloc::collections::BTreeMap;
use core::marker::Copy;

use encoding::all::ISO_8859_1;
use encoding::{DecoderTrap, Encoding};

use crate::parsers::{extract, Endian, FromSlice};
use crate::record::StateMetadata;
use crate::EtError;
use crate::{impl_reader, impl_record};
use crate::record::Value;

fn decode_iso_8859(raw: &[u8]) -> Result<String, EtError> {
    ISO_8859_1.decode(raw, DecoderTrap::Ignore).map_err(|e| e.into_owned().into())
}

/// State of the Chemstation REG parser
#[derive(Clone, Copy, Debug, Default)]
pub struct ChemstationRegState {



}

impl StateMetadata for ChemstationRegState {
    fn header(&self) -> Vec<&str> {
        vec!["time", "intensity"]
    }
}

impl<'b: 's, 's> FromSlice<'b, 's> for ChemstationRegState {
    type State = ();

    fn parse(
        buf: &[u8],
        eof: bool,
        consumed: &mut usize,
        _state: &mut Self::State,
    ) -> Result<bool, EtError> {
        let con = &mut 0;
        let header = extract::<&[u8]>(buf, con, &mut 45)?;

        if header[25] != b'A' {
            return Err(EtError::from("Version of REG file is too new"));
        }
        let n_sections = u16::extract(&header[38..], &Endian::Little)?;

        // TODO: parse multiple sections

        let n_records = extract::<u32>(buf, con, &mut Endian::Little)? as usize;

        let mut records = Vec::with_capacity(n_records);
        for _ in 0..n_records {
            let _ = extract::<u16>(buf, con, &mut Endian::Little)?;
            let record_type = extract::<u16>(buf, con, &mut Endian::Little)?;
            let record_len = extract::<u32>(buf, con, &mut Endian::Little)? as usize;
            let _ = extract::<u32>(buf, con, &mut Endian::Little)?;
            let record_id = extract::<u32>(buf, con, &mut Endian::Little)?;
            records.push((record_type, record_len, record_id))
        }

        let mut names: BTreeMap<u32, String> = BTreeMap::new();
        let mut metadata: BTreeMap<u32, Value> = BTreeMap::new();
        for (record_type, mut record_len, record_id) in records {
            let record_data = extract::<&[u8]>(buf, con, &mut record_len)?;
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
                    let record_id = u32::extract(&record_data[35..], &Endian::Little)?;
                    let _ = names.insert(record_id, decode_iso_8859(record_data[14..30].split(|c| *c == 0).next().unwrap_or(&record_data[14..30]))?);
                },
                // part of a linked list
                1538 => {
                    if record_data.len() != 39 {
                        return Err(EtError::from("Data type 1538 was an unexpected size"));
                    }
                    let _ = names.insert(record_id, decode_iso_8859(&record_data[14..35])?);
                    let _ = metadata.insert(record_id, u32::extract(&record_data[35..], &Endian::Little)?.into());
                },
                // another part of a linked list with a table reference
                1539 => {
                    if record_data.len() != 39 {
                        return Err(EtError::from("Data type 1539 was an unexpected size"));
                    }
                    let id = u32::extract(&record_data[35..], &Endian::Little)?;
                    let _ = names.insert(id, decode_iso_8859(&record_data[14..35])?);
                    // no data?
                },
                // table of values
                1793 => {
					let n_rows = u16::extract(&record_data[4..], &Endian::Little)?;
					let n_columns = u16::extract(&record_data[16..], &Endian::Little)?;
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
                        return Err(EtError::from("Array was undersized"));
                    }
                    let n_points = record_data.len() / 4 - 1;
                    let mut data: Vec<Value> = Vec::with_capacity(n_points);
                    for ix in 0..n_points {
                        data.push(u32::extract(&record_data[4 * ix + 4..], &Endian::Little)?.into());
                    }
                    let _ = metadata.insert(record_id, data.into());
                },
                _ => { },
            }
        }

        Ok(true)
    }

    fn get(
        &mut self,
        buf: &'b [u8],
        state: &'s Self::State,
    ) -> Result<(), EtError> {
        Ok(())
    }
}

/// Record
#[derive(Clone, Copy, Debug, Default)]
pub struct ChemstationRegRecord {
    point: f64
}

impl<'b: 's, 's> FromSlice<'b, 's> for ChemstationRegRecord {
    type State = ChemstationRegState;

    fn parse(
        buf: &[u8],
        eof: bool,
        consumed: &mut usize,
        _state: &mut Self::State,
    ) -> Result<bool, EtError> {
        Ok(false)
    }

    fn get(
        &mut self,
        buf: &'b [u8],
        state: &'s Self::State,
    ) -> Result<(), EtError> {
        Ok(())
    }
}

impl_record!(ChemstationRegRecord: point);

impl_reader!(ChemstationRegReader, ChemstationRegRecord, ChemstationRegRecord, ChemstationRegState, ());

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chemstation_reg_reader() -> Result<(), EtError> {
        let rb: &[u8] = include_bytes!("../../../tests/data/chemstation_mwd.d/LCDIAG.REG");
        let mut reader = ChemstationRegReader::new(rb, None)?;

        let mut n_recs = 1;
        while reader.next()?.is_some() {
            n_recs += 1;
        }
        assert_eq!(n_recs, 5);
        Ok(())
    }
}
