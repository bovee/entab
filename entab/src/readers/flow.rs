use alloc::string::{String, ToString};
use alloc::vec::Vec;
use alloc::{format, str};
use core::default::Default;

use crate::buffer::ReadBuffer;
use crate::parsers::{Endian, FromBuffer};
use crate::readers::RecordReader;
use crate::record::Value;
use crate::EtError;

/// A single key-value pair from the text segment of an FCS file.
#[derive(Clone, Debug, PartialEq)]
struct FcsHeaderKeyValue<'a>(String, &'a str);

impl<'r> FromBuffer<'r> for FcsHeaderKeyValue<'r> {
    type State = u8;

    fn get(rb: &'r mut ReadBuffer, delim: Self::State) -> Result<Self, EtError> {
        let mut i = 0;
        let mut temp = None;
        let (key_end, value_end) = loop {
            if i + 2 >= rb.len() {
                if !rb.eof() {
                    rb.refill()?;
                } else if temp != None && rb[i + 1] == delim {
                    break (temp.unwrap(), i + 1);
                } else {
                    return Err(EtError::new("FCS header ended abruptly"));
                }
            }
            if rb[i] == delim {
                if rb[i + 1] == delim {
                    // skip consectutive delimiters
                    i += 1;
                } else if temp != None {
                    break (temp.unwrap(), i);
                } else {
                    temp = Some(i);
                }
            }
            i += 1;
        };
        let temp = rb.consume(value_end + 1);
        let key = str::from_utf8(&temp[..key_end])?.to_ascii_uppercase();
        let value = str::from_utf8(&temp[key_end + 1..value_end])?;
        Ok(FcsHeaderKeyValue(key, value))
    }
}

fn str_to_int(s: &[u8]) -> Result<u64, EtError> {
    Ok(str::from_utf8(s)?.trim().parse()?)
}

#[derive(Clone, Debug, Default)]
struct FcsParam {
    size: i8,
    range: u64,
    short_name: String,
    long_name: String,
}

/// State of an FcsReader.
///
/// Note that the state is primarily derived from the TEXT segment of the file.
#[derive(Clone, Debug)]
pub struct FcsState {
    params: Vec<FcsParam>,
    endian: Endian,
    data_type: char,
    next_data: Option<usize>,
    n_events_left: usize,
}

impl<'r> FromBuffer<'r> for FcsState {
    type State = ();

    fn get(rb: &'r mut ReadBuffer, _state: Self::State) -> Result<Self, EtError> {
        let mut params = Vec::new();
        let mut endian = Endian::Little;
        let mut data_type = 'F';
        let mut next_data = None;
        let mut n_events_left = 0;

        let start_pos = rb.get_byte_pos() as usize;

        let magic = rb.extract::<&[u8]>(10)?;
        if &magic[..3] != b"FCS" {
            return Err(EtError::new("FCS file has invalid header"));
        }

        // get the offsets to the different data
        let text_start = str_to_int(rb.extract::<&[u8]>(8)?)?;
        let text_end = str_to_int(rb.extract::<&[u8]>(8)?)?;
        let mut data_start = str_to_int(rb.extract::<&[u8]>(8)?)?;
        let mut data_end = str_to_int(rb.extract::<&[u8]>(8)?)?;
        let _ = rb.extract::<&[u8]>(16)?;
        // let analysis_start = rb.extract::<AsciiInt>(8)?.0 as usize;
        // let analysis_end = rb.extract::<AsciiInt>(8)?.0 as usize;

        let _ = rb.extract::<&[u8]>(text_start as usize - 58 - start_pos)?;
        let delim: u8 = rb.extract(Endian::Little)?;
        while rb.get_byte_pos() < text_end {
            let FcsHeaderKeyValue(key, value) = rb.extract(delim)?;
            match (key.as_ref(), value) {
                ("$BEGINDATA", v) => {
                    let data_start_value = v.trim().parse::<u64>()?;
                    if data_start_value > 0 {
                        data_start = data_start_value;
                    }
                }
                ("$ENDDATA", v) => {
                    let data_end_value = v.trim().parse::<u64>()?;
                    if data_end_value > 0 {
                        data_end = data_end_value;
                    }
                }
                ("$NEXTDATA", v) => {
                    let next_value = v.trim().parse()?;
                    if next_value > 0 {
                        next_data = Some(next_value);
                    }
                }
                ("$BYTEORD", "4,3,2,1") => endian = Endian::Big,
                ("$BYTEORD", "2,1") => endian = Endian::Big,
                ("$DATATYPE", "A") => data_type = 'A',
                ("$DATATYPE", "D") => data_type = 'D',
                ("$DATATYPE", "F") => data_type = 'F',
                ("$DATATYPE", "I") => data_type = 'I',
                ("$DATATYPE", v) => {
                    return Err(EtError::new(format!("Unknown FCS $DATATYPE {}", v)))
                }
                ("$MODE", "L") => {}
                ("$MODE", "C") | ("$MODE", "U") => {
                    return Err(EtError::new("FCS histograms not yet supported ($MODE=C/U)"))
                }
                ("$MODE", v) => return Err(EtError::new(format!("Unknown FCS $MODE {}", v))),
                ("$TOT", v) => n_events_left = v.trim().parse()?,
                ("$PAR", v) => {
                    let n_params = v.trim().parse()?;
                    if n_params < params.len() {
                        return Err(EtError::new(format!("Declared number of params ({}) is less than the observed number of params ({})", n_params, params.len())));
                    }
                    params.resize_with(n_params, FcsParam::default)
                }
                (k, v) if k.starts_with("$P") && k.ends_with(&['B', 'N', 'R', 'S'][..]) => {
                    let mut i: usize = k[2..k.len() - 1].parse()?;
                    i -= 1; // params are numbered from 1
                    if i >= params.len() {
                        params.resize_with(i, FcsParam::default)
                    }
                    if k.ends_with('B') {
                        if v == "*" {
                            // this should only be true for $DATATYPE=A
                            params[i].size = -1;
                        } else {
                            params[i].size = v.trim().parse()?;
                        }
                    } else if k.ends_with('N') {
                        params[i].short_name = v.to_string();
                    } else if k.ends_with('R') {
                        let range: u64 = v.trim().parse()?;
                        if range.count_ones() != 1 {
                            return Err(EtError::new("Range values must be a power of 2"));
                        }
                        params[i].range = range;
                    } else if k.ends_with('S') {
                        params[i].long_name = v.to_string();
                    }
                }
                _ => {}
            }
        }
        // get anything between the end of the text segment and the start of the data segment
        let _ = rb.extract::<&[u8]>((data_start - text_end) as usize)?;

        if data_end < data_start {
            return Err(EtError::new("Invalid end from data segment"));
        }
        // TODO: check that the datatypes and Bs match up

        Ok(FcsState {
            params,
            endian,
            data_type,
            next_data,
            n_events_left,
        })
    }
}

/// A reader for Flow Cytometry Standard (FCS) data.
///
/// Because the fields of a FCS record are variable, this reader only
/// implements the `next_record` interface and doesn't have its own
/// specialize FcsRecord.
///
/// For a more detailed specification of the FCS format, see:
/// https://www.bioconductor.org/packages/release/bioc/vignettes/flowCore/inst/doc/fcs3.html
#[derive(Debug)]
pub struct FcsReader<'r> {
    rb: ReadBuffer<'r>,
    state: FcsState,
}

impl<'r> FcsReader<'r> {
    /// Create a new FcsReader from the ReadBuffer provided.
    pub fn new(mut rb: ReadBuffer<'r>, _params: ()) -> Result<Self, EtError> {
        let state = rb.extract::<FcsState>(())?;
        Ok(FcsReader { rb, state })
    }
}

impl<'r> RecordReader for FcsReader<'r> {
    fn headers(&self) -> Vec<String> {
        let mut headers = Vec::new();
        for param in &self.state.params {
            headers.push(param.short_name.clone());
        }
        headers
    }

    fn next_record(&mut self) -> Result<Option<Vec<Value>>, EtError> {
        if self.state.n_events_left == 0 {
            if let Some(next_data) = self.state.next_data {
                let _ = self
                    .rb
                    .extract::<&[u8]>(next_data - self.rb.get_byte_pos() as usize)?;
                self.state = self.rb.extract(())?;
            } else {
                return Ok(None);
            }
        }

        let mut record = Vec::with_capacity(self.state.params.len());
        for param in &self.state.params {
            record.push(match self.state.data_type {
                'A' if param.size > 0 => {
                    let n = self.rb.extract::<&[u8]>(param.size as usize)?;
                    str::from_utf8(n)?.trim().parse::<f64>()?.into()
                }
                'A' if param.size < 0 => {
                    return Err(EtError::new(
                        "Delimited-ASCII number datatypes are not yet supported",
                    ));
                }
                'D' => self.rb.extract::<f64>(self.state.endian)?.into(),
                'F' => self.rb.extract::<f32>(self.state.endian)?.into(),
                'I' => {
                    let value: u64 = match param.size {
                        8 => self.rb.extract::<u8>(self.state.endian)?.into(),
                        16 => self.rb.extract::<u16>(self.state.endian)?.into(),
                        32 => self.rb.extract::<u32>(self.state.endian)?.into(),
                        64 => self.rb.extract::<u64>(self.state.endian)?,
                        x => return Err(EtError::new(format!("Unknown param size {}", x))),
                    };
                    let range_mask = param.range - 1;
                    (value & range_mask).into()
                }
                _ => panic!("Data type is in an unknown state"),
            })
        }
        self.state.n_events_left -= 1;
        Ok(Some(record))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fcs_header_kv_parser() -> Result<(), EtError> {
        let mut rb = ReadBuffer::from_slice(b"test/key/");
        let test_parse: FcsHeaderKeyValue = rb.extract(b'/')?;
        assert_eq!(test_parse, FcsHeaderKeyValue("TEST".to_string(), "key"));

        let mut rb = ReadBuffer::from_slice(b"test/key");
        assert!(rb.extract::<FcsHeaderKeyValue>(b'/').is_err());

        let mut rb = ReadBuffer::from_slice(b" ");
        assert!(rb.extract::<FcsHeaderKeyValue>(b'/').is_err());

        let mut rb = ReadBuffer::from_slice(b"//");
        assert!(rb.extract::<FcsHeaderKeyValue>(b'/').is_err());

        // super pathological case that should probably never occur? (since it
        // would imply the previous ending delim was before this start delim)
        let mut rb = ReadBuffer::from_slice(b"/ /");
        let test_parse: FcsHeaderKeyValue = rb.extract(b'/')?;
        assert_eq!(test_parse, FcsHeaderKeyValue("".to_string(), " "));
        Ok(())
    }

    #[test]
    fn test_fcs_reader() -> Result<(), EtError> {
        let rb = ReadBuffer::from_slice(include_bytes!(
            "../../tests/data/HTS_BD_LSR_II_Mixed_Specimen_001_D6_D06.fcs"
        ));
        let mut reader = FcsReader::new(rb, ())?;

        let record = reader
            .next_record()?
            .expect("Reader returns at least one value");
        assert_eq!(record.len(), 11);

        let mut n_recs = 1;
        while let Some(_) = reader.next_record()? {
            n_recs += 1;
        }
        assert_eq!(n_recs, 14945);
        Ok(())
    }
}
