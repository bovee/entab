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

#[extendr]
fn as_data_frame(reader: &mut Reader) -> Result<Robj> {
    let mut data: Vec<Vec<Robj>> = vec![vec![]; reader.header_names.len()];
    while let Some(record) = reader.reader.next_record().map_err(to_r)? {
        for (ix, v) in record.into_iter().enumerate() {
            data[ix].push(value_to_robj(v));
        }
    }
    let mut vectors: Vec<Robj> = vec![];
    for v in data {
        vectors.push(v.into());
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
