use std::collections::BTreeMap;
use std::fs::File;

use entab_base::error::EtError;
use entab_base::readers::{get_reader, RecordReader};
use entab_base::record::Value;
use extendr_api::prelude::*;

#[allow(clippy::needless_pass_by_value)]
fn to_r(err: EtError) -> Error {
    err.to_string().into()
}

fn value_to_robj(value: Value) -> Robj {
    match value {
        Value::Null => ().into(),
        Value::Boolean(b) => b.into(),
        Value::Datetime(dt) => lang!("as.POSIXlt", dt.timestamp(), origin = "1970-01-01"),
        Value::Float(f) => f.into(),
        Value::Integer(i) => i.into(),
        Value::String(s) => s.as_ref().into(),
        Value::List(l) => {
            let mut values = Vec::new();
            for v in l {
                values.push(value_to_robj(v));
            }
            List::from_values(values).into()
        }
        Value::Record(r) => {
            let mut names = Vec::new();
            let mut values = Vec::new();
            for (key, value) in r {
                names.push(key);
                values.push(value_to_robj(value));
            }
            List::from_names_and_values(names, values).into()
        }
    }
}

struct Reader {
    parser: String,
    header_names: Vec<String>,
    reader: Box<dyn RecordReader>,
}

#[extendr]
impl Reader {
    #[allow(clippy::new_ret_no_self)]
    fn new(filename: &str, parser: &str) -> Result<Robj> {
        let file = File::open(filename).map_err(|e| Error::from(e.to_string()))?;
        let parser = if parser.is_empty() {
            None
        } else {
            Some(parser)
        };
        let mut params = BTreeMap::new();
        params.insert("filename".to_string(), Value::String(filename.into()));
        let (reader, parser_used) = get_reader(file, parser, Some(params)).map_err(to_r)?;
        let header_names = reader.headers();
        Ok(Reader {
            parser: parser_used.to_string(),
            header_names,
            reader,
        }
        .into())
    }

    fn parser(&self) -> &str {
        &self.parser
    }

    fn headers(&self) -> Vec<String> {
        self.reader.headers()
    }

    fn metadata(&self) -> Robj {
        let metadata = self.reader.metadata();
        let mut names = Vec::new();
        let mut values = Vec::new();
        for (key, value) in metadata {
            names.push(key);
            values.push(value_to_robj(value));
        }
        List::from_names_and_values(names, values).into()
    }

    fn next(&mut self) -> Result<Robj> {
        if let Some(record) = self.reader.next_record().map_err(to_r)? {
            let mut values = Vec::new();
            for v in record {
                values.push(value_to_robj(v));
            }
            Ok(List::from_names_and_values(&self.header_names, values).into())
        } else {
            Ok(().into())
        }
    }
}

pub enum ValueList {
    Null(usize),
    Boolean(Vec<bool>),
    Float(Vec<f64>),
    Integer(Vec<i64>),
    String(Vec<String>),
    Misc(Vec<Robj>),
}

#[extendr]
fn as_data_frame(reader: &mut Reader) -> Result<Robj> {
    let mut data: Vec<ValueList> = Vec::new();
    if let Some(first) = reader.reader.next_record().map_err(to_r)? {
        for v in first {
            data.push(match v {
                Value::Null => ValueList::Null(1),
                Value::Boolean(b) => ValueList::Boolean(vec![b]),
                Value::Float(f) => ValueList::Float(vec![f]),
                Value::Integer(i) => ValueList::Integer(vec![i]),
                Value::String(s) => ValueList::String(vec![s.to_string()]),
                x => ValueList::Misc(vec![value_to_robj(x)]),
            });
        }
        while let Some(record) = reader.reader.next_record().map_err(to_r)? {
            for (ix, v) in record.into_iter().enumerate() {
                match (&mut data[ix], v) {
                    (ValueList::Null(x), Value::Null) => *x += 1,
                    (ValueList::Boolean(v), Value::Boolean(b)) => v.push(b),
                    (ValueList::Float(v), Value::Float(f)) => v.push(f),
                    (ValueList::Integer(v), Value::Integer(i)) => v.push(i),
                    (ValueList::String(v), Value::String(s)) => v.push(s.to_string()),
                    (ValueList::Misc(v), x) => v.push(value_to_robj(x)),
                    _ => panic!("Tried to append wrong data type"),
                }
            }
        }
    } else {
        for _ in &reader.header_names {
            data.push(ValueList::Null(0));
        }
    }

    let mut vectors: Vec<Robj> = vec![];
    for v in data {
        vectors.push(match v {
            ValueList::Null(x) => vec![r!(NULL); x].into(),
            ValueList::Boolean(v) => v.iter().collect_robj(),
            ValueList::Float(v) => v.iter().collect_robj(),
            ValueList::Integer(v) => v.iter().collect_robj(),
            ValueList::String(v) => v.iter().collect_robj(),
            ValueList::Misc(v) => v.into(),
        });
    }
    let obj: Robj = List::from_names_and_values(&reader.header_names, &vectors).into();
    obj.set_attrib(
        row_names_symbol(),
        (1i32..=vectors[0].len() as i32).collect_robj(),
    )?;
    obj.set_class(&["data.frame"])?;
    Ok(obj)
}

extendr_module! {
    mod entab;
    impl Reader;
    fn as_data_frame;
}
