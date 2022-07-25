use alloc::borrow::Cow;
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::convert::TryFrom;

use chrono::{NaiveDate, NaiveDateTime};
use serde::{Serialize, Serializer};

use crate::error::EtError;

/// For a given state struct, the metadata associated with that struct.
///
/// Primarily used to generate the corresponding metadata in the
/// `RecordReader` trait.
pub trait StateMetadata {
    /// Metadata about the current state of the parser
    fn metadata(&self) -> BTreeMap<String, Value> {
        BTreeMap::new()
    }

    /// The fields in the associated struct
    fn header(&self) -> Vec<&str>;
}

impl StateMetadata for () {
    fn header(&self) -> Vec<&str> {
        Vec::new()
    }
}

/// Autogenerates the conversion from a struct into the matching `Vec` of
/// headers and the corresponding `Vec` of `Value`s to allow decomposing
/// these raw structs into a common Record system that allows abstracting
/// over different file formats.
#[macro_export]
macro_rules! impl_record {
    ($type:ty : $($key:ident),* ) => {
        impl<'r> From<$type> for ::alloc::vec::Vec<$crate::record::Value<'r>> {
            fn from(record: $type) -> Self {
                ::alloc::vec![$(record.$key.into(),)*]
            }
        }
    };
    ($type:ty : $($key:ident)+ ) => { record!($($key),+) };
}

/// An arbitrary serializable value
///
/// Similar to the value types in `toml-rs` and `serde-json`, but in addition
/// we need to derive other methods for e.g. converting into something
/// displayable in a TSV so we couldn't use those.
#[derive(PartialEq, Clone, Debug)]
pub enum Value<'a> {
    /// A null value; all other types are considered implicitly nullable
    Null,
    /// A true/false value
    Boolean(bool),
    /// A date with associated time
    Datetime(NaiveDateTime),
    /// A floating point number
    Float(f64),
    /// An integer
    Integer(i64),
    /// A string/textual data
    String(Cow<'a, str>),
    /// A list of `Value`s (not well supported yet)
    List(Vec<Value<'a>>),
    /// A record mapping keys to `Value`s
    Record(BTreeMap<String, Value<'a>>),
}

impl<'a> Value<'a> {
    /// Converts an ISO-8601 formated date into a `Value::Datetime`
    ///
    /// # Errors
    /// If the string can't be interpreted as a date, an error is returned.
    pub fn from_iso_date(string: &str) -> Result<Self, EtError> {
        let datetime = NaiveDateTime::parse_from_str(string, "%+")
            .map_err(|e| EtError::from(e.to_string()))?;
        Ok(Self::Datetime(datetime))
    }

    /// If the Value is a String, return the string.
    ///
    /// # Errors
    /// If the value isn't a string, an error is returned.
    pub fn into_string(self) -> Result<String, EtError> {
        if let Value::String(s) = self {
            return Ok(s.into_owned());
        }
        Err(EtError::from("Value was not a string"))
    }
}

impl<'a, T: Into<Value<'a>>> From<Option<T>> for Value<'a> {
    fn from(x: Option<T>) -> Self {
        match x {
            None => Value::Null,
            Some(s) => s.into(),
        }
    }
}

impl<'a> From<bool> for Value<'a> {
    fn from(x: bool) -> Self {
        Value::Boolean(x)
    }
}

impl<'a> From<f32> for Value<'a> {
    fn from(x: f32) -> Self {
        Value::Float(f64::from(x))
    }
}

impl<'a> From<f64> for Value<'a> {
    fn from(x: f64) -> Self {
        Value::Float(x)
    }
}

impl<'a> From<u8> for Value<'a> {
    fn from(x: u8) -> Self {
        Value::Integer(i64::from(x))
    }
}

impl<'a> From<u16> for Value<'a> {
    fn from(x: u16) -> Self {
        Value::Integer(i64::from(x))
    }
}

impl<'a> From<i32> for Value<'a> {
    fn from(x: i32) -> Self {
        Value::Integer(i64::from(x))
    }
}

impl<'a> From<u32> for Value<'a> {
    fn from(x: u32) -> Self {
        Value::Integer(i64::from(x))
    }
}

impl<'a> From<i64> for Value<'a> {
    fn from(x: i64) -> Self {
        Value::Integer(x)
    }
}

impl<'a> From<u64> for Value<'a> {
    fn from(x: u64) -> Self {
        if x.leading_zeros() == 0 {
            // handle u64 -> i64 overflow by saturating; maybe someday this should be a try_from?
            Value::Integer(i64::MAX)
        } else {
            Value::Integer(i64::try_from(x).unwrap())
        }
    }
}

impl<'a> From<Cow<'a, [u8]>> for Value<'a> {
    fn from(x: Cow<'a, [u8]>) -> Self {
        Value::String(match x {
            Cow::Borrowed(b) => String::from_utf8_lossy(b),
            Cow::Owned(o) => Cow::Owned(String::from_utf8_lossy(&o).into_owned()),
        })
    }
}

impl<'a> From<&'a [u8]> for Value<'a> {
    fn from(x: &'a [u8]) -> Self {
        Value::String(String::from_utf8_lossy(x))
    }
}

impl<'a> From<Vec<u8>> for Value<'a> {
    fn from(x: Vec<u8>) -> Self {
        Value::String(Cow::Owned(String::from_utf8_lossy(&x).into_owned()))
    }
}

impl<'a> From<Cow<'a, str>> for Value<'a> {
    fn from(x: Cow<'a, str>) -> Self {
        Value::String(x)
    }
}

impl<'a> From<&'a str> for Value<'a> {
    fn from(x: &'a str) -> Self {
        Value::String(x.into())
    }
}

impl<'a> From<String> for Value<'a> {
    fn from(x: String) -> Self {
        Value::String(x.into())
    }
}

impl<'a> From<NaiveDateTime> for Value<'a> {
    fn from(d: NaiveDateTime) -> Self {
        Value::Datetime(d)
    }
}

impl<'a> From<NaiveDate> for Value<'a> {
    fn from(d: NaiveDate) -> Self {
        Value::Datetime(d.and_hms(0, 0, 0))
    }
}

impl<'a> From<&'a [String]> for Value<'a> {
    fn from(value: &'a [String]) -> Self {
        let mut rec = Vec::with_capacity(value.len());
        for v in value {
            let bv: &str = v.as_ref();
            rec.push(bv.into());
        }
        Value::List(rec)
    }
}

impl<'a> From<Vec<String>> for Value<'a> {
    fn from(value: Vec<String>) -> Self {
        let mut rec = Vec::with_capacity(value.len());
        for v in value {
            rec.push(v.into());
        }
        Value::List(rec)
    }
}

impl<'a> From<Vec<Value<'a>>> for Value<'a> {
    fn from(value: Vec<Value<'a>>) -> Self {
        Value::List(value)
    }
}

impl<'a> Serialize for Value<'a> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match *self {
            Value::Null => serializer.serialize_none(),
            Value::Boolean(b) => serializer.serialize_bool(b),
            Value::Datetime(ref s) => s.serialize(serializer),
            Value::Float(f) => serializer.serialize_f64(f),
            Value::Integer(i) => serializer.serialize_i64(i),
            Value::List(ref a) => a.serialize(serializer),
            Value::Record(ref t) => t.serialize(serializer),
            Value::String(ref s) => serializer.serialize_str(s),
        }
    }
}
