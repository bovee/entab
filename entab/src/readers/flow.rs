use alloc::borrow::{Cow, ToOwned};
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use alloc::{format, str};
use core::convert::{TryFrom, TryInto};
use core::default::Default;

use chrono::{NaiveDate, NaiveTime};

use crate::buffer::ReadBuffer;
use crate::parsers::{extract, extract_opt, Endian, FromSlice, Skip};
use crate::readers::RecordReader;
use crate::record::Value;
use crate::EtError;

/// A single key-value pair from the text segment of an FCS file.
#[derive(Clone, Debug, Default, PartialEq)]
struct FcsHeaderKeyValue<'a>(String, Cow<'a, str>);

impl<'r> FromSlice<'r> for FcsHeaderKeyValue<'r> {
    type State = (u8, usize, usize);

    fn parse(
        rb: &[u8],
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
            if i + 2 >= rb.len() {
                if temp != None && rb[i + 1] == *delim {
                    *key_end = temp.unwrap();
                    break i + 1;
                } else if !eof {
                    return Err(EtError::from("Incomplete FCS header").incomplete());
                }
                return Err("FCS header ended abruptly".into());
            }
            if rb[i] == *delim && temp != None {
                if rb[i + 1] == *delim {
                    // skip consectutive delimiters
                    i += 1;
                } else {
                    *key_end = temp.unwrap();
                    break i;
                }
            } else if rb[i] == *delim {
                if rb[i + 1] == *delim {
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

    fn get(&mut self, buf: &'r [u8], (_, _, key_end): &Self::State) -> Result<(), EtError> {
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

impl<'r> FromSlice<'r> for FcsState {
    type State = ();

    fn parse(
        rb: &[u8],
        _eof: bool,
        consumed: &mut usize,
        _state: &mut Self::State,
    ) -> Result<bool, EtError> {
        let con = &mut 0;

        let magic = extract::<&[u8]>(rb, con, 10)?;
        if &magic[..3] != b"FCS" {
            return Err("FCS file has invalid header".into());
        }

        // get the offsets to the different data
        let text_start = usize::try_from(str_to_int(extract::<&[u8]>(rb, con, 8)?)?)?;
        let text_end = usize::try_from(str_to_int(extract::<&[u8]>(rb, con, 8)?)?)?;
        if text_end < text_start {
            return Err("Invalid end from text segment".into());
        }
        let mut data_start = str_to_int(extract::<&[u8]>(rb, con, 8)?)?;
        let mut data_end = str_to_int(extract::<&[u8]>(rb, con, 8)?)?;
        if text_start < 58 {
            return Err("Bad FCS text start offset".into());
        }
        // skip the analysis_start/analysis_end values
        let _ = extract::<Skip>(rb, con, 16)?;
        let _ = extract::<Skip>(rb, con, text_start - 58)?;
        let delim: u8 = extract(rb, con, Endian::Little)?;
        while let Some(FcsHeaderKeyValue(key, value)) = extract_opt::<FcsHeaderKeyValue>(
            rb,
            false,
            con,
            (delim, text_end.saturating_sub(*con), 0),
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
        }
        // get anything between the end of the text segment and the start of the data segment
        let _ = extract::<Skip>(rb, con, usize::try_from(data_start)? - *con)?;

        *consumed += *con;
        Ok(true)
    }

    fn get(&mut self, rb: &'r [u8], _state: &Self::State) -> Result<(), EtError> {
        let mut params = Vec::new();
        let mut endian = Endian::Little;
        let mut data_type = 'F';
        let mut next_data = None;
        let mut n_events_left = 0;

        let con = &mut 0;

        let magic = extract::<&[u8]>(rb, con, 10)?;
        if &magic[..3] != b"FCS" {
            return Err("FCS file has invalid header".into());
        }
        let mut metadata = BTreeMap::new();

        // get the offsets to the different data
        let text_start = usize::try_from(str_to_int(extract::<&[u8]>(rb, con, 8)?)?)?;
        let text_end = usize::try_from(str_to_int(extract::<&[u8]>(rb, con, 8)?)?)?;
        let mut data_start = str_to_int(extract::<&[u8]>(rb, con, 8)?)?;
        let mut data_end = str_to_int(extract::<&[u8]>(rb, con, 8)?)?;
        if text_start < 58 {
            return Err("Bad FCS text start offset".into());
        }
        let _ = extract::<Skip>(rb, con, 16)?;
        // let analysis_start = rb.extract::<AsciiInt>(8)?.0 as usize;
        // let analysis_end = rb.extract::<AsciiInt>(8)?.0 as usize;
        let _ = extract::<Skip>(rb, con, text_start - 58)?;
        let delim: u8 = extract(rb, con, Endian::Little)?;
        let mut date = NaiveDate::from_yo(2000, 1);
        let mut time = NaiveTime::from_num_seconds_from_midnight(0, 0);
        while let Some(FcsHeaderKeyValue(key, value)) = extract_opt::<FcsHeaderKeyValue>(
            rb,
            false,
            con,
            (delim, text_end.saturating_sub(*con), 0),
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

/// A reader for Flow Cytometry Standard (FCS) data.
///
/// Because the fields of a FCS record are variable, this reader only
/// implements the `next_record` interface and doesn't have its own
/// specialize `FcsRecord`.
///
/// For a more detailed specification of the FCS format, see:
/// <https://www.bioconductor.org/packages/release/bioc/vignettes/flowCore/inst/doc/fcs3.html>
#[derive(Debug)]
pub struct FcsReader<'r> {
    rb: ReadBuffer<'r>,
    state: FcsState,
}

impl<'r> FcsReader<'r> {
    /// Create a new `FcsReader` from the `ReadBuffer` provided.
    pub fn new<B>(data: B, params: ()) -> Result<Self, EtError>
    where
        B: TryInto<ReadBuffer<'r>>,
        EtError: From<<B as TryInto<ReadBuffer<'r>>>::Error>,
    {
        let mut rb = data.try_into()?;
        if let Some(state) = rb.next::<FcsState>(params)? {
            Ok(FcsReader { rb, state })
        } else {
            Err("Could not read FCS headers".into())
        }
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

    fn metadata(&self) -> BTreeMap<String, Value> {
        self.state.metadata.clone()
    }

    fn next_record(&mut self) -> Result<Option<Vec<Value>>, EtError> {
        let con = &mut 0;
        if self.state.n_events_left == 0 {
            if let Some(next_data) = self.state.next_data {
                let _ = extract::<Skip>(self.rb.as_ref(), con, next_data - self.rb.consumed)?;
                self.state = extract(self.rb.as_ref(), con, ())?;
            } else {
                return Ok(None);
            }
        }

        let mut record = Vec::with_capacity(self.state.params.len());
        // TODO: need to handle incompletes here
        for param in &self.state.params {
            record.push(match self.state.data_type {
                'A' if param.size > 0 => {
                    let n = extract::<&[u8]>(self.rb.as_ref(), con, param.size as usize)?;
                    str::from_utf8(n)?.trim().parse::<f64>()?.into()
                }
                'A' if param.size < 0 => {
                    return Err("Delimited-ASCII number datatypes are not yet supported".into());
                }
                'D' => extract::<f64>(self.rb.as_ref(), con, self.state.endian)?.into(),
                'F' => extract::<f32>(self.rb.as_ref(), con, self.state.endian)?.into(),
                'I' => {
                    let value: u64 = match param.size {
                        8 => extract::<u8>(self.rb.as_ref(), con, self.state.endian)?.into(),
                        16 => extract::<u16>(self.rb.as_ref(), con, self.state.endian)?.into(),
                        32 => extract::<u32>(self.rb.as_ref(), con, self.state.endian)?.into(),
                        64 => extract::<u64>(self.rb.as_ref(), con, self.state.endian)?,
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
            });
        }
        self.state.n_events_left -= 1;
        self.rb.record_pos += 1;
        Ok(Some(record))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fcs_header_kv_parser() -> Result<(), EtError> {
        let rb = &b"test/key/"[..];
        let test_parse = extract::<FcsHeaderKeyValue>(rb, &mut 0, (b'/', 100, 0))?;
        assert_eq!(
            test_parse,
            FcsHeaderKeyValue("TEST".to_string(), "key".into())
        );

        let rb = b"test/key";
        assert!(extract::<FcsHeaderKeyValue>(rb, &mut 0, (b'/', 100, 0)).is_err());

        let rb = b" ";
        assert!(extract::<FcsHeaderKeyValue>(rb, &mut 0, (b'/', 100, 0)).is_err());

        let rb = b"//";
        assert!(extract::<FcsHeaderKeyValue>(rb, &mut 0, (b'/', 100, 0)).is_err());

        // super pathological case that should probably never occur? (since it
        // would imply the previous ending delim was before this start delim)
        let rb = b"/ /";
        let test_parse = extract::<FcsHeaderKeyValue>(rb, &mut 0, (b'/', 100, 0))?;
        assert_eq!(test_parse, FcsHeaderKeyValue("".to_string(), " ".into()));

        Ok(())
    }

    #[test]
    fn test_fcs_reader() -> Result<(), EtError> {
        let rb: &[u8] =
            include_bytes!("../../tests/data/HTS_BD_LSR_II_Mixed_Specimen_001_D6_D06.fcs");
        let mut reader = FcsReader::new(rb, ())?;
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

    #[test]
    fn test_fcs_reader_metadata() -> Result<(), EtError> {
        let rb: &[u8] =
            include_bytes!("../../tests/data/HTS_BD_LSR_II_Mixed_Specimen_001_D6_D06.fcs");
        let reader = FcsReader::new(&rb[..], ())?;
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
        let rb: &[u8] = b"FCS3.1  \n\n\n0\n\n\n\n\n\n0\n\n\n\n\n\n\n \n\n\n0\n\n\n\n \n\n\n0\n\nCS3.1  \n\n\n0\n\n\n\n\n;";
        assert!(FcsReader::new(rb, ()).is_err());
        Ok(())
    }
}
