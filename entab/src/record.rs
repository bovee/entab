use alloc::borrow::Cow;
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use serde::{Serialize, Serializer};

use crate::utils::string::replace_tabs;
use crate::EtError;

pub trait RecHeader {
    fn header() -> Vec<String>;
}

#[macro_export]
macro_rules! impl_record {
    ($type:ty : $($key:ident),* ) => {
        impl<'r> $crate::record::RecHeader for $type {
            fn header() -> ::alloc::vec::Vec<::alloc::string::String> {
                use ::alloc::string::ToString;
                let mut header = ::alloc::vec::Vec::new();
                $(
                    header.push(stringify!($key).to_string());
                )*
                header
            }
        }

        impl<'r> From<$type> for ::alloc::vec::Vec<$crate::record::Value> {
            fn from(record: $type) -> Self {
                let mut list = ::alloc::vec::Vec::new();
                $(
                    list.push(record.$key.into());
                )*
                list
            }
        }
    };
    ($type:ty : $($key:ident)+ ) => { record!($($key),+) };
}

#[derive(PartialEq, Clone, Debug)]
pub enum Value {
    Null,
    Boolean(bool),
    Datetime(String),
    Float(f64),
    Integer(i64),
    List(Vec<Value>),
    Record(BTreeMap<String, Value>),
    String(String),
}

impl Value {
    pub fn write_for_tsv<W>(&self, mut write: W) -> Result<(), EtError>
    where
        W: FnMut(&[u8]) -> Result<(), EtError>,
    {
        match self {
            Value::Null => write(b"null"),
            Value::Boolean(true) => write(b"true"),
            Value::Boolean(false) => write(b"false"),
            Value::Datetime(s) => write(s.as_bytes()),
            Value::Float(v) => write(format!("{}", v).as_bytes()),
            Value::Integer(v) => write(format!("{}", v).as_bytes()),
            Value::List(_) => unimplemented!("No writer for lists yet"),
            Value::Record(_) => unimplemented!("No writer for records yet"),
            Value::String(s) => write(&replace_tabs(s.as_bytes(), b'|')),
        }
    }
}

impl<T: Into<Value>> From<Option<T>> for Value {
    fn from(x: Option<T>) -> Self {
        match x {
            None => Value::Null,
            Some(s) => s.into(),
        }
    }
}

impl From<bool> for Value {
    fn from(x: bool) -> Self {
        Value::Boolean(x)
    }
}

impl From<f64> for Value {
    fn from(x: f64) -> Self {
        Value::Float(x)
    }
}

impl From<u8> for Value {
    fn from(x: u8) -> Self {
        Value::Integer(i64::from(x))
    }
}

impl From<u16> for Value {
    fn from(x: u16) -> Self {
        Value::Integer(i64::from(x))
    }
}

impl From<i32> for Value {
    fn from(x: i32) -> Self {
        Value::Integer(i64::from(x))
    }
}

impl From<u32> for Value {
    fn from(x: u32) -> Self {
        Value::Integer(i64::from(x))
    }
}

impl From<i64> for Value {
    fn from(x: i64) -> Self {
        Value::Integer(x)
    }
}

impl From<u64> for Value {
    fn from(x: u64) -> Self {
        // there's probably a better solution here
        Value::Integer(x as i64)
    }
}

impl<'a> From<Cow<'a, [u8]>> for Value {
    fn from(x: Cow<'a, [u8]>) -> Self {
        Value::String(String::from_utf8_lossy(&x).into_owned())
    }
}

impl<'a> From<&'a [u8]> for Value {
    fn from(x: &'a [u8]) -> Self {
        Value::String(String::from_utf8_lossy(x).into_owned())
    }
}

impl From<&str> for Value {
    fn from(x: &str) -> Self {
        Value::String(x.to_string())
    }
}

impl From<String> for Value {
    fn from(x: String) -> Self {
        Value::String(x)
    }
}

impl Serialize for Value {
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
