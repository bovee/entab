use alloc::collections::BTreeMap;
use alloc::{format, str};
use alloc::string::{String, ToString};
use core::char::{decode_utf16, REPLACEMENT_CHARACTER};

use chrono::NaiveDateTime;

use crate::parsers::{Endian, FromSlice};
use crate::record::Value;
use crate::EtError;

#[derive(Clone, Debug, Default)]
/// Metadata consistly found in Chemstation file formats
pub struct ChemstationMetadata {
    /// The time the run started collecting at in minutes
    pub start_time: f64,
    /// The time the run stopped collecting at in minutes
    pub end_time: f64,
    /// Name of the signal record (specifically used for e.g. MWD traces)
    pub signal_name: String,
    /// Absolute correction to be applied to all data points
    pub offset_correction: f64,
    /// Scaling correction to be applied to all data points
    pub mult_correction: f64,
    /// In what order this run was performed
    pub sequence: u16,
    /// The vial number this run was performed from
    pub vial: u16,
    /// The replicate number of this run
    pub replicate: u16,
    /// The name of the sample
    pub sample: String,
    /// The description of the sample
    pub description: String,
    /// The name of the operator
    pub operator: String,
    /// The date the sample was run
    pub run_date: Option<NaiveDateTime>,
    /// The instrument the sample was run on
    pub instrument: String,
    /// The method the instrument ran
    pub method: String,
    /// The units of the y scale.
    pub y_units: String,
}

impl ChemstationMetadata {
    /// Parse the header to extract the metadata
    pub fn from_header(header: &[u8]) -> Result<Self, EtError> {
        if header.len() < 256 {
            return Err(EtError::from(
                "All Chemstation header needs to be at least 256 bytes long",
            )
            .incomplete());
        }
        let version = u32::extract(&header[248..], &Endian::Big)?;

        let required_length = match version {
            2 | 102 => 512,
            30 | 31 | 81 => 652,
            131 => 4000,
            130 | 179 => 4800,
            _ => usize::MAX,
        };
        if header.len() < required_length {
            return Err(EtError::from(format!(
                "Chemstation {} header needs to be at least {} bytes long",
                version, required_length
            ))
            .incomplete());
        }

        // 258..260 - 0 or 1
        // 260..264 - 0 or large int (/60000?)
        // 254..268 - 9 or 13
        // only in 179 and 130
        // 290..294 - 63429.0 - f32 / 930051 - i32
        // 294..298 - 0 / -22385
        // 298..302 - repeat of 290
        // 302..306 - repeat of 294

        // There's another data section at 4100 that
        // has duplicates of some of these values?

        let sequence = u16::extract(&header[252..], &Endian::Big)?;
        let vial = u16::extract(&header[254..], &Endian::Big)?;
        let replicate = u16::extract(&header[256..], &Endian::Big)?;

        let sample = match version {
            0..=102 => get_pascal(&header[24..24 + 60], "sample")?,
            _ => get_utf16_pascal(&header[858..]),
        };
        let description = match version {
            0..=102 => get_pascal(&header[86..86 + 60], "description")?,
            _ => "".to_string(),
        };
        let operator = match version {
            0..=102 => get_pascal(&header[148..148 + 28], "operator")?,
            _ => get_utf16_pascal(&header[1880..]),
        };
        let instrument = match version {
            0..=102 => get_pascal(&header[208..228], "instrument")?,
            _ => get_utf16_pascal(&header[2492..]),
        };
        let method = match version {
            0..=102 => get_pascal(&header[228..], "method")?,
            _ => get_utf16_pascal(&header[2574..]),
        };

        let signal_name = match version {
            30 | 31 | 81 => get_pascal(&header[596..596 + 40], "signal_name")?,
            130 | 179 => get_utf16_pascal(&header[4213..]),
            _ => "".to_string(),
        };

        let offset_correction = match version {
            30 | 31 | 81 => f64::extract(&header[636..], &Endian::Big)?,
            _ => 0.,
        };
        let mult_correction = match version {
            30 | 31 | 81 => f64::extract(&header[644..], &Endian::Big)?,
            131 => f64::extract(&header[3085..3093], &Endian::Big)?,
            130 | 179 => f64::extract(&header[4732..4770], &Endian::Big)?,
            _ => 1.,
        };
        let start_time = match version {
            2 | 30 | 31 | 81 | 102 | 130 | 131 => {
                i32::extract(&header[282..], &Endian::Big)? as f64 / 60000.
            }
            179 => f32::extract(&header[282..], &Endian::Big)? as f64 / 60000.,
            _ => 0.,
        };
        let end_time = match version {
            2 | 30 | 31 | 81 | 102 | 130 | 131 => {
                i32::extract(&header[286..], &Endian::Big)? as f64 / 60000.
            }
            179 => f32::extract(&header[286..], &Endian::Big)? as f64 / 60000.,
            _ => 0.,
        };
        let y_units = match version {
            81 => get_pascal(&header[244..244 + 64], "y_units")?,
            131 => get_utf16_pascal(&header[3093..]),
            130 | 179 => get_utf16_pascal(&header[4172..]),
            _ => "".to_string(),
        };

        // We need to detect the date format before we can convert into a
        // NaiveDateTime; not sure the format even maps to the file type
        // (it may be computer-dependent?)
        let raw_run_date = match version {
            0..=102 => get_pascal(&header[178..178 + 60], "run_date")?,
            130 | 131 | 179 => get_utf16_pascal(&header[2391..]),
            _ => "".to_string(),
        };
        let run_date = if let Ok(d) =
            NaiveDateTime::parse_from_str(raw_run_date.as_ref(), "%d-%b-%y, %H:%M:%S")
        {
            // format in MWD
            Some(d)
        } else if let Ok(d) =
            NaiveDateTime::parse_from_str(raw_run_date.as_ref(), "%d %b %y %l:%M %P")
        {
            // format in MS
            Some(d)
        } else if let Ok(d) =
            NaiveDateTime::parse_from_str(raw_run_date.as_ref(), "%d %b %y %l:%M %P %z")
        {
            // format in MS with timezone
            Some(d)
        } else if let Ok(d) =
            NaiveDateTime::parse_from_str(raw_run_date.as_ref(), "%m/%d/%y %I:%M:%S %p")
        {
            // format in FID
            Some(d)
        } else {
            None
        };

        Ok(Self {
            start_time,
            end_time,
            signal_name,
            offset_correction,
            mult_correction,
            sequence,
            vial,
            replicate,
            sample,
            description,
            operator,
            run_date,
            instrument,
            method,
            y_units,
        })
    }
}

impl<'r> From<&ChemstationMetadata> for BTreeMap<String, Value<'r>> {
    fn from(metadata: &ChemstationMetadata) -> Self {
        let mut map = BTreeMap::new();
        drop(map.insert("start_time".to_string(), metadata.start_time.into()));
        drop(map.insert("end_time".to_string(), metadata.end_time.into()));
        drop(map.insert(
            "signal_name".to_string(),
            metadata.signal_name.clone().into(),
        ));
        drop(map.insert(
            "offset_correction".to_string(),
            metadata.offset_correction.into(),
        ));
        drop(map.insert(
            "mult_correction".to_string(),
            metadata.mult_correction.into(),
        ));
        drop(map.insert("sequence".to_string(), metadata.sequence.into()));
        drop(map.insert("vial".to_string(), metadata.vial.into()));
        drop(map.insert("replicate".to_string(), metadata.replicate.into()));
        drop(map.insert("sample".to_string(), metadata.sample.clone().into()));
        drop(map.insert(
            "description".to_string(),
            metadata.description.clone().into(),
        ));
        drop(map.insert("operator".to_string(), metadata.operator.clone().into()));
        drop(map.insert("run_date".to_string(), metadata.run_date.into()));
        drop(map.insert("instrument".to_string(), metadata.instrument.clone().into()));
        drop(map.insert("method".to_string(), metadata.method.clone().into()));
        drop(map.insert("y_units".to_string(), metadata.y_units.clone().into()));
        map
    }
}

fn get_utf16_pascal(data: &[u8]) -> String {
    let iter = (1..=2 * usize::from(data[0]))
        .step_by(2)
        .map(|i| u16::from_le_bytes([data[i], data[i + 1]]));
    decode_utf16(iter)
        .map(|r| r.unwrap_or(REPLACEMENT_CHARACTER))
        .collect::<String>()
}

fn get_pascal(data: &[u8], field_name: &'static str) -> Result<String, EtError> {
    let string_len = usize::from(data[0]);
    if string_len > data.len() {
        return Err(EtError::from(format!("Invalid {} length", field_name)).incomplete());
    }
    Ok(str::from_utf8(&data[1..1 + string_len])?.trim().to_string())
}
