mod util;

use std::fs::File;
use std::io::Read;

use entab_base::buffer::ReadBuffer;
use entab_base::compression::decompress;
use entab_base::error::EtError;
use entab_base::readers::{get_reader, RecordReader};
use entab_base::record::Value;
use extendr_api::*;

use util::{unwrap_result, vec_to_list};

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
            vec_to_list(&values, None)
        }
        Value::Record(r) => {
            let mut names = Vec::new();
            let mut values = Vec::new();
            for (key, value) in r.into_iter() {
                names.push(key);
                values.push(value_to_robj(value));
            }
            vec_to_list(&values, Some(&names.into()))
        }
    }
}

struct Reader {
    parser: String,
    header_names: Robj,
    reader: Box<dyn RecordReader>,
}

fn new_reader(filename: &str, parser: &str) -> Result<Reader, EtError> {
    let stream: Box<dyn Read> = Box::new(File::open(filename)?);
    let (reader, filetype, _) = decompress(stream)?;
    let buffer = ReadBuffer::new(reader)?;

    let parser_name = if parser == "" {
        filetype.to_parser_name()
    } else {
        parser
    };
    let reader = get_reader(parser_name, buffer)?;
    let header_names = reader.headers().into();
    Ok(Reader {
        parser: parser_name.to_string(),
        header_names,
        reader,
    })
}

fn next_reader(reader: &mut Reader) -> Result<Robj, EtError> {
    if let Some(record) = reader.reader.next_record()? {
        let mut values = Vec::new();
        for v in record {
            values.push(value_to_robj(v));
        }
        Ok(vec_to_list(&values, Some(&reader.header_names)))
    } else {
        Ok(().into())
    }
}

#[extendr]
impl Reader {
    fn new(filename: &str, parser: &str) -> Robj {
        // TODO: move this back inline once extendr supports returning Result
        unwrap_result(new_reader(filename, parser))
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
        for (key, value) in metadata.into_iter() {
            names.push(key);
            values.push(value_to_robj(value));
        }
        vec_to_list(&values, Some(&names.into()))
    }

    fn next(&mut self) -> Robj {
        // TODO: move this back inline once extendr supports returning Result
        unwrap_result(next_reader(self))
    }
}

extendr_module! {
    mod entab;
    impl Reader;
}
