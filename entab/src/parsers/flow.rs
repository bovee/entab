use alloc::borrow::{Cow, ToOwned};
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use alloc::{format, str};
use core::convert::TryFrom;
use core::default::Default;

use chrono::{NaiveDate, NaiveTime};

use crate::impl_reader;
use crate::parsers::common::Skip;
use crate::parsers::{extract, extract_opt, Endian, FromSlice};
use crate::record::{StateMetadata, Value};
use crate::EtError;

/// A single key-value pair from the text segment of an FCS file.
#[derive(Clone, Debug, Default, PartialEq)]
struct FcsHeaderKeyValue<'a>(String, Cow<'a, str>);

impl<'b: 's, 's> FromSlice<'b, 's> for FcsHeaderKeyValue<'s> {
    type State = (u8, usize, usize);

    fn parse(
        buf: &[u8],
        eof: bool,
        consumed: &mut usize,
        (delim, text_left, key_end): &mut Self::State,
    ) -> Result<bool, EtError> {
        let mut i = 0;
        let mut temp = None;
        let value_end = loop {
            if i > *text_left {
                return Ok(false);
            }
            if i + 2 >= buf.len() {
                if i + 1 >= buf.len() {
                    return Err(EtError::from("Incomplete key in FCS header").incomplete());
                } else if temp != None && buf[i + 1] == *delim {
                    *key_end = temp.unwrap();
                    break i + 1;
                } else if !eof {
                    return Err(EtError::from("Incomplete FCS header").incomplete());
                }
                return Err("FCS header ended abruptly".into());
            }
            if buf[i] == *delim && temp != None {
                if buf[i + 1] == *delim {
                    // skip consectutive delimiters
                    i += 1;
                } else {
                    *key_end = temp.unwrap();
                    break i;
                }
            } else if buf[i] == *delim {
                if buf[i + 1] == *delim {
                    // The spec says this should be parsed as an escaped
                    // delimiter in the key, but I've never seen that so
                    // we parse it as an empty value (which I have seen
                    // in Applied Biosystems files).
                    *key_end = i;
                    break i + 1;
                }
                temp = Some(i);
            }
            i += 1;
        };
        *consumed += value_end + 1;
        Ok(true)
    }

    fn get(&mut self, buf: &'b [u8], (_, _, key_end): &'s Self::State) -> Result<(), EtError> {
        self.0 = str::from_utf8(&buf[..*key_end])?.to_ascii_uppercase();
        self.1 = String::from_utf8_lossy(&buf[*key_end + 1..buf.len() - 1]);
        Ok(())
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

/// State of an `FcsReader`.
///
/// Note that the state is primarily derived from the TEXT segment of the file.
#[derive(Clone, Debug, Default)]
pub struct FcsState {
    params: Vec<FcsParam>,
    endian: Endian,
    data_type: char,
    next_data: Option<usize>,
    n_events_left: usize,
    metadata: BTreeMap<String, Value<'static>>,
}

impl StateMetadata for FcsState {
    fn metadata(&self) -> BTreeMap<String, Value> {
        self.metadata.clone()
    }

    /// The fields in the associated struct
    fn header(&self) -> Vec<&str> {
        let mut headers = Vec::new();
        for param in &self.params {
            headers.push(param.short_name.as_ref());
        }
        headers
    }
}

impl<'b: 's, 's> FromSlice<'b, 's> for FcsState {
    type State = ();

    fn parse(
        buf: &[u8],
        _eof: bool,
        consumed: &mut usize,
        _state: &mut Self::State,
    ) -> Result<bool, EtError> {
        let con = &mut 0;

        let magic = extract::<&[u8]>(buf, con, &mut 10)?;
        if &magic[..3] != b"FCS" {
            return Err("FCS file has invalid header".into());
        }

        // get the offsets to the different data
        let text_start = usize::try_from(str_to_int(extract::<&[u8]>(buf, con, &mut 8)?)?)?;
        let text_end = usize::try_from(str_to_int(extract::<&[u8]>(buf, con, &mut 8)?)?)?;
        if text_end < text_start {
            return Err("Invalid end from text segment".into());
        }
        let mut data_start = str_to_int(extract::<&[u8]>(buf, con, &mut 8)?)?;
        let mut data_end = str_to_int(extract::<&[u8]>(buf, con, &mut 8)?)?;
        if text_start < 58 {
            return Err("Bad FCS text start offset".into());
        }
        // skip the analysis_start/analysis_end values
        let _ = extract::<Skip>(buf, con, &mut 16)?;
        let _ = extract::<Skip>(buf, con, &mut (text_start - 58))?;
        let delim: u8 = extract(buf, con, &mut Endian::Little)?;
        while let Some(FcsHeaderKeyValue(key, value)) = extract_opt::<FcsHeaderKeyValue>(
            buf,
            false,
            con,
            &mut (delim, text_end.saturating_sub(*con), 0),
        )? {
            match (key.as_ref(), value.as_ref()) {
                ("$BEGINDATA", v) => {
                    let data_start_value = v.trim().parse::<u64>()?;
                    if data_start_value > 0 && data_start == 0 {
                        data_start = data_start_value;
                    }
                }
                ("$ENDDATA", v) => {
                    let data_end_value = v.trim().parse::<u64>()?;
                    if data_end_value > 0 && data_end == 0 {
                        data_end = data_end_value;
                    }
                }
                _ => {}
            }
        }
        if data_end < data_start {
            return Err("Invalid end from data segment".into());
        } else if data_start < text_end as u64 {
            return Err("Data segment can not start before text segment ends".into());
        }
        // get anything between the end of the text segment and the start of the data segment
        if usize::try_from(data_start)? < *con {
            return Err(EtError::from("Ran out of data before data segment started").incomplete());
        }
        let _ = extract::<Skip>(buf, con, &mut (usize::try_from(data_start)? - *con))?;

        *consumed += *con;
        Ok(true)
    }

    fn get(&mut self, buf: &'b [u8], _state: &'s Self::State) -> Result<(), EtError> {
        let mut params = Vec::new();
        let mut endian = Endian::Little;
        let mut data_type = 'F';
        let mut next_data = None;
        let mut n_events_left = 0;

        let con = &mut 0;

        let magic = extract::<&[u8]>(buf, con, &mut 10)?;
        if &magic[..3] != b"FCS" {
            return Err("FCS file has invalid header".into());
        }
        let mut metadata = BTreeMap::new();

        // get the offsets to the different data
        let text_start = usize::try_from(str_to_int(extract::<&[u8]>(buf, con, &mut 8)?)?)?;
        let text_end = usize::try_from(str_to_int(extract::<&[u8]>(buf, con, &mut 8)?)?)?;
        let mut data_start = str_to_int(extract::<&[u8]>(buf, con, &mut 8)?)?;
        let mut data_end = str_to_int(extract::<&[u8]>(buf, con, &mut 8)?)?;
        if text_start < 58 {
            return Err("Bad FCS text start offset".into());
        }
        let _ = extract::<Skip>(buf, con, &mut 16)?;
        // let analysis_start = buf.extract::<AsciiInt>(8)?.0 as usize;
        // let analysis_end = buf.extract::<AsciiInt>(8)?.0 as usize;
        let _ = extract::<Skip>(buf, con, &mut (text_start - 58))?;
        let delim: u8 = extract(buf, con, &mut Endian::Little)?;
        let mut date = NaiveDate::from_yo(2000, 1);
        let mut time = NaiveTime::from_num_seconds_from_midnight(0, 0);
        while let Some(FcsHeaderKeyValue(key, value)) = extract_opt::<FcsHeaderKeyValue>(
            buf,
            false,
            con,
            &mut (delim, text_end.saturating_sub(*con), 0),
        )? {
            match (key.as_ref(), value.as_ref()) {
                ("$BEGINDATA", v) => {
                    let data_start_value = v.trim().parse::<u64>()?;
                    if data_start_value > 0 && data_start == 0 {
                        data_start = data_start_value;
                    }
                }
                ("$ENDDATA", v) => {
                    let data_end_value = v.trim().parse::<u64>()?;
                    if data_end_value > 0 && data_end == 0 {
                        data_end = data_end_value;
                    }
                }
                ("$NEXTDATA", v) => {
                    let next_value: usize = v.trim().parse()?;
                    if next_value > 0 {
                        next_data = Some(next_value);
                    }
                }
                ("$BYTEORD", "4,3,2,1" | "2, 1") => endian = Endian::Big,
                ("$DATATYPE", "A") => data_type = 'A',
                ("$DATATYPE", "D") => data_type = 'D',
                ("$DATATYPE", "F") => data_type = 'F',
                ("$DATATYPE", "I") => data_type = 'I',
                ("$DATATYPE", v) => return Err(format!("Unknown FCS $DATATYPE {}", v).into()),
                ("$MODE", "L") => {}
                ("$MODE", "C" | "U") => {
                    return Err("FCS histograms not yet supported ($MODE=C/U)".into())
                }
                ("$MODE", v) => return Err(format!("Unknown FCS $MODE {}", v).into()),
                ("$TOT", v) => n_events_left = v.trim().parse()?,
                ("$BTIM", v) => {
                    // TODO: sometimes there's a fractional (/60) part after the last colon
                    // that we should include in the time too
                    let hms = v
                        .trim()
                        .split(':')
                        .take(3)
                        .map(ToOwned::to_owned)
                        .collect::<Vec<String>>()
                        .join(":");
                    if let Ok(t) = NaiveTime::parse_from_str(&hms, "%H:%M:%S") {
                        time = t;
                    }
                }
                ("$CELLS", v) => {
                    drop(metadata.insert("specimen".into(), v.to_string().into()));
                }
                ("$DATE", v) => {
                    // "DD-MM-YYYY"
                    // "YYYY-mmm-DD"
                    if let Ok(d) = NaiveDate::parse_from_str(v.trim(), "%d-%b-%y") {
                        // FCS2.0 only had a two-digit year, e.g. 01-JAN-20).
                        date = d;
                    } else if let Ok(d) = NaiveDate::parse_from_str(v.trim(), "%d-%b-%Y") {
                        // FCS3.0 and 3.1 are supposed to be e.g. 01-JAN-2020.
                        date = d;
                    } else if let Ok(d) = NaiveDate::parse_from_str(v.trim(), "%Y-%b-%d") {
                        // non-standard FCS3.0?
                        date = d;
                    } else if let Ok(d) = NaiveDate::parse_from_str(v.trim(), "%d-%m-%Y") {
                        // one weird Partec FCS2.0 file had this
                        date = d;
                    }
                }
                ("$INST", v) => {
                    drop(metadata.insert("instrument".into(), v.to_string().into()));
                }
                ("$OP", v) => {
                    drop(metadata.insert("operator".into(), v.to_string().into()));
                }
                ("$PROJ", v) => {
                    drop(metadata.insert("project".into(), v.to_string().into()));
                }
                ("$SMNO", v) => {
                    drop(metadata.insert("specimen_number".into(), v.to_string().into()));
                }
                ("$SRC", v) => {
                    drop(metadata.insert("specimen_source".into(), v.to_string().into()));
                }
                ("$PAR", v) => {
                    let n_params = v.trim().parse()?;
                    if n_params < params.len() {
                        return Err(format!("Declared number of params ({}) is less than the observed number of params ({})", n_params, params.len()).into());
                    }
                    params.resize_with(n_params, FcsParam::default);
                }
                (k, v) if k.starts_with("$P") && k.ends_with(&['B', 'N', 'R', 'S'][..]) => {
                    let mut i: usize = k[2..k.len() - 1].parse()?;
                    i -= 1; // params are numbered from 1
                    if i >= params.len() {
                        params.resize_with(i + 1, FcsParam::default);
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
                        // some yahoos put ranges for $DATATYPE=F in their
                        // files as the floats so we have to parse as float
                        // here and convert into
                        let range = v.trim().parse::<f64>()?;
                        params[i].range = range.ceil() as u64;
                    } else if k.ends_with('S') {
                        params[i].long_name = v.to_string();
                    }
                }
                _ => {}
            }
        }
        drop(metadata.insert("date".into(), date.and_time(time).into()));

        // check that the datatypes and params match up
        for p in &params {
            match data_type {
                'D' => {
                    if p.size != 64 {
                        return Err("Param size must be 64 for $DATATYPE=D".into());
                    }
                }
                'F' => {
                    if p.size != 32 {
                        return Err("Param size must be 32 for $DATATYPE=F".into());
                    }
                }
                _ => {}
            }
        }

        self.params = params;
        self.endian = endian;
        self.data_type = data_type;
        self.next_data = next_data;
        self.n_events_left = n_events_left;
        self.metadata = metadata;
        Ok(())
    }
}

/// A record from a FCS file.
///
/// Because the fields of a FCS record are variable, this stores them
/// as two sets of `Vec`s.
///
/// For a more detailed specification of the FCS format, see:
/// <https://www.bioconductor.org/packages/release/bioc/vignettes/flowCore/inst/doc/fcs3.html>
#[derive(Debug, Default)]
pub struct FcsRecord<'r> {
    /// A list of the values for the current FCS scan. See the associated state for their names.
    pub values: Vec<Value<'r>>,
}

impl<'b: 's, 's> FromSlice<'b, 's> for FcsRecord<'s> {
    type State = FcsState;

    fn parse(
        buf: &[u8],
        eof: bool,
        consumed: &mut usize,
        state: &mut Self::State,
    ) -> Result<bool, EtError> {
        if state.n_events_left == 0 {
            if let Some(next_data) = state.next_data {
                let con = &mut 0;
                let _ = extract::<Skip>(buf, con, &mut (next_data - *consumed))?;
                if !FcsState::parse(buf, eof, consumed, &mut ())? {
                    return Ok(false);
                }
                FcsState::get(state, buf, &())?;
                *consumed += *con;
            } else {
                return Ok(false);
            }
        }

        let mut data_size: usize = 0;
        for param in &state.params {
            data_size += match state.data_type {
                'A' if param.size > 0 => param.size as usize,
                'A' if param.size < 0 => {
                    return Err("Delimited-ASCII number datatypes are not yet supported".into());
                }
                'D' => 8,
                'F' => 4,
                'I' => {
                    if param.size % 8 != 0 {
                        return Err(format!("Unknown param size {}", param.size).into());
                    }
                    param.size as usize / 8
                }
                _ => panic!("Data type is in an unknown state"),
            };
        }
        if data_size > buf.len() {
            return Err(EtError::from("Record was incomplete").incomplete());
        }
        state.n_events_left -= 1;
        *consumed += data_size;
        Ok(true)
    }

    fn get(&mut self, buf: &'b [u8], state: &'s Self::State) -> Result<(), EtError> {
        if self.values.len() != state.params.len() {
            self.values.resize(state.params.len(), Value::Null);
        }
        // TODO: need to handle incompletes here
        let con = &mut 0;
        for (ix, param) in state.params.iter().enumerate() {
            self.values[ix] = match state.data_type {
                'A' if param.size > 0 => {
                    let n = extract::<&[u8]>(buf, con, &mut (param.size as usize))?;
                    str::from_utf8(n)?.trim().parse::<f64>()?.into()
                }
                'A' if param.size < 0 => {
                    return Err("Delimited-ASCII number datatypes are not yet supported".into());
                }
                'D' => extract::<f64>(buf, con, &mut state.endian.clone())?.into(),
                'F' => extract::<f32>(buf, con, &mut state.endian.clone())?.into(),
                'I' => {
                    let value: u64 = match param.size {
                        8 => extract::<u8>(buf, con, &mut state.endian.clone())?.into(),
                        16 => extract::<u16>(buf, con, &mut state.endian.clone())?.into(),
                        32 => extract::<u32>(buf, con, &mut state.endian.clone())?.into(),
                        64 => extract::<u64>(buf, con, &mut state.endian.clone())?,
                        x => return Err(format!("Unknown param size {}", x).into()),
                    };
                    if value > param.range && param.range > 0 {
                        if param.range.count_ones() != 1 {
                            return Err("Only ranges of power 2 can mask values".into());
                        }
                        let range_mask = param.range - 1;
                        (value & range_mask).into()
                    } else {
                        value.into()
                    }
                }
                _ => panic!("Data type is in an unknown state"),
            };
        }
        Ok(())
    }
}

impl<'r> From<FcsRecord<'r>> for Vec<Value<'r>> {
    fn from(record: FcsRecord<'r>) -> Self {
        record.values
    }
}

impl_reader!(FcsReader, FcsRecord, FcsRecord<'r>, FcsState, ());

#[cfg(test)]
mod tests {
    use super::*;
    use crate::readers::RecordReader;

    #[test]
    fn test_fcs_header_kv_parser() -> Result<(), EtError> {
        let buf = &b"test/key/"[..];
        let mut state = (b'/', 100, 0);
        let test_parse = extract::<FcsHeaderKeyValue>(buf, &mut 0, &mut state)?;
        assert_eq!(
            test_parse,
            FcsHeaderKeyValue("TEST".to_string(), "key".into())
        );

        let buf = b"test/key";
        assert!(extract::<FcsHeaderKeyValue>(buf, &mut 0, &mut (b'/', 100, 0)).is_err());

        let buf = b" ";
        assert!(extract::<FcsHeaderKeyValue>(buf, &mut 0, &mut (b'/', 100, 0)).is_err());

        let buf = b"//";
        assert!(extract::<FcsHeaderKeyValue>(buf, &mut 0, &mut (b'/', 100, 0)).is_err());

        // super pathological case that should probably never occur? (since it
        // would imply the previous ending delim was before this start delim)
        let buf = b"/ /";
        let mut state = (b'/', 100, 0);
        let test_parse = extract::<FcsHeaderKeyValue>(buf, &mut 0, &mut state)?;
        assert_eq!(test_parse, FcsHeaderKeyValue("".to_string(), " ".into()));

        Ok(())
    }

    #[test]
    fn test_fcs_reader() -> Result<(), EtError> {
        let buf: &[u8] =
            include_bytes!("../../tests/data/HTS_BD_LSR_II_Mixed_Specimen_001_D6_D06.fcs");
        let mut reader = FcsReader::new(buf, None)?;
        assert_eq!(
            reader.headers(),
            [
                "FSC-A",
                "FSC-H",
                "FSC-W",
                "SSC-A",
                "SSC-H",
                "SSC-W",
                "FITC-A",
                "PerCP-Cy5-5-A",
                "AmCyan-A",
                "PE-TxRed YG-A",
                "Time"
            ]
        );

        let record = reader.next()?.expect("Reader returns at least one value");
        assert_eq!(record.values.len(), 11);

        let mut n_recs = 1;
        while let Some(_) = reader.next()? {
            n_recs += 1;
        }
        assert_eq!(n_recs, 14945);
        Ok(())
    }

    #[test]
    fn test_fcs_reader_metadata() -> Result<(), EtError> {
        let buf: &[u8] =
            include_bytes!("../../tests/data/HTS_BD_LSR_II_Mixed_Specimen_001_D6_D06.fcs");
        let reader = FcsReader::new(&buf[..], None)?;
        let metadata = reader.metadata();
        assert_eq!(metadata["specimen_source"], "Specimen_001".into());
        assert_eq!(
            metadata["date"],
            NaiveDate::from_ymd(2012, 10, 26).and_hms(18, 08, 10).into()
        );
        Ok(())
    }

    #[test]
    fn test_fcs_bad_fuzzes() -> Result<(), EtError> {
        let test_data: &[u8] = b"FCS3.1  \n\n\n0\n\n\n\n\n\n0\n\n\n\n\n\n\n \n\n\n0\n\n\n\n \n\n\n0\n\nCS3.1  \n\n\n0\n\n\n\n\n;";
        assert!(FcsReader::new(test_data, None).is_err());

        let test_data: &[u8] = b"FCS3.1  \n0\t\t\t\t\t\t77777777777777777777777777777777\t\x1a@@\x1a{\n\x1a\t00vyyy\t\t0\t0\t77777yy\tyyyyyyyy\0\0\0\0\0\0\0\0\0\0\0\0\0\x0000\t\t0\t0:\0\0\x05\x1a\n{\t17777yy\t\x1a\n{\t17777777777yy\t";
        assert!(FcsReader::new(test_data, None).is_err());

        let test_data: &[u8] = b"FCS3.1  \n0\t\t\t\t\t\t7777\t\t\t\t\t\t00000000007777777777\0\0\x007777y\t0\tH\0\0\0\0\0\x007777777\t\t\ty7777777\t\t\tyyy\t0\tH\0\0\0\0\x007777777\t\t\0\x00777777yy\t0\tH\0\0\0\0\0\x007777777\t\t";
        assert!(FcsReader::new(test_data, None).is_err());

        let test_data: &[u8] = b"FCS3.1  \n0\t\t\t\t\t\t7777\t\t\t\t\t\t00000077777707777yyyy77777\t0000006692\x1a\t0\x01\0\0\0-\0D`\0\x000\t\t*\tyyyy77777\t777\0\0-\0D`\0\x000\t\t*\tyyyy77777\t77777\t77777\t77777\t";
        assert!(FcsReader::new(test_data, None).is_err());

        Ok(())
    }
}
