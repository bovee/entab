#![allow(clippy::needless_option_as_deref, clippy::used_underscore_binding)]
mod raw_io_wrapper;

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{Cursor, Read};

use entab_base::error::EtError;
use entab_base::readers::{get_reader, RecordReader};
use entab_base::record::Value;
use pyo3::class::{PyIterProtocol, PyObjectProtocol};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple};
use pyo3::{create_exception, exceptions};

use crate::raw_io_wrapper::RawIoWrapper;

create_exception!(entab, EntabError, exceptions::PyException);

fn to_py(err: EtError) -> PyErr {
    // TODO: somehow bind err.byte and err.record in here too?
    let res = EntabError::new_err(err.to_string());
    // we could technically just take an `&EtError` here, but the function signature is nicer with
    // a `EtError` so we have to drop it here to make clippy happy
    drop(err);
    res
}

/// Map a Value into a `PyObject`
fn py_from_value(value: Value, py: Python) -> PyResult<PyObject> {
    Ok(match value {
        Value::Null => py.None().as_ref(py).into(),
        Value::Boolean(b) => b.to_object(py),
        Value::Datetime(d) => {
            // NB: For files without timezone data (and all NaiveDateTime?),
            // .format("%+") panics. So timezone is omitted
            d.format("%Y-%m-%dT%H:%M:%S%.f").to_string().to_object(py)
            // TODO: it would be nice to use Python's built-in datetime, but that doesn't appear to
            // be abi3-compatible right now
            //            let timestamp = d.timestamp_millis() as f64 / 1000.;
            //            pyo3::types::PyDateTime::from_timestamp(py, timestamp, None)?.to_object(py)
        }
        Value::Float(v) => v.to_object(py),
        Value::Integer(v) => v.to_object(py),
        Value::String(s) => s.to_object(py),
        Value::List(l) => {
            let list = PyList::empty(py);
            for item in l {
                list.append(py_from_value(item, py)?)?;
            }
            list.to_object(py)
        }
        Value::Record(_) => {
            return Err(EntabError::new_err("record subelements unimplemented"));
        }
    })
}

// TODO: remove the unsendable; by wrapping reader in an Arc?
#[pyclass(unsendable)]
#[pyo3(text_signature = "(/, data=None, filename=None, parser=None)")]
pub struct Reader {
    #[pyo3(get)]
    parser: String,
    record_class: Py<PyAny>,
    reader: Box<dyn RecordReader>,
}

#[pyproto]
impl PyIterProtocol for Reader {
    fn __iter__(slf: PyRefMut<Self>) -> PyResult<PyObject> {
        let gil = Python::acquire_gil();
        let py = gil.python();
        let val: PyObject = slf.into_py(py);
        Ok(val.clone_ref(py))
    }

    fn __next__(mut slf: PyRefMut<Self>) -> PyResult<Option<Py<PyAny>>> {
        let gil = Python::acquire_gil();
        let py = gil.python();
        let rec = if let Some(val) = slf.reader.next_record().map_err(to_py)? {
            let mut data = Vec::with_capacity(val.len());
            for field in val {
                data.push(py_from_value(field, py)?);
            }
            let tup = PyTuple::new(py, data);
            slf.record_class.as_ref(py).call1(tup)?
        } else {
            return Ok(None);
        };
        Ok(Some(rec.into()))
    }
}

#[pymethods]
impl Reader {
    #[new]
    #[args(data = "None", filename = "None", parser = "None")]
    fn new(data: Option<&PyAny>, filename: Option<&str>, parser: Option<&str>) -> PyResult<Self> {
        let mut params = BTreeMap::new();
        let stream: Box<dyn Read> = match (data, filename) {
            (Some(d), None) => {
                if let Ok(bytes) = d.extract::<Vec<u8>>() {
                    Box::new(Cursor::new(bytes))
                } else if let Ok(string) = d.extract::<String>() {
                    Box::new(Cursor::new(string.into_bytes()))
                } else if d.hasattr("read")? {
                    Box::new(RawIoWrapper::new(d))
                } else {
                    return Err(EntabError::new_err(
                        "`data` must be str, bytes or implement `read`",
                    ));
                }
            }
            (None, Some(f)) => {
                params.insert("filename".to_string(), Value::String(f.into()));
                Box::new(File::open(f)?)
            }
            _ => {
                return Err(EntabError::new_err(
                    "One and only one of `data` or `filename` must be provided",
                ))
            }
        };
        let (reader, parser_used) = get_reader(stream, parser, Some(params)).map_err(to_py)?;
        let gil = Python::acquire_gil();
        let py = gil.python();

        let headers: Vec<String> = reader
            .headers()
            .iter()
            .map(|h| h.replace(" ", "_").replace("-", "_"))
            .collect();
        let collections = PyModule::import(py, "collections")?;
        let record_class = collections
            .getattr("namedtuple")?
            .call1(("Record", headers))?
            .into();

        Ok(Reader {
            parser: parser_used.to_string(),
            record_class,
            reader,
        })
    }

    #[getter]
    pub fn get_headers(&self) -> PyResult<Vec<String>> {
        Ok(self.reader.headers())
    }

    #[getter]
    pub fn get_metadata(&self) -> PyResult<PyObject> {
        let gil = Python::acquire_gil();
        let py = gil.python();

        let dict = PyDict::new(py);
        for (key, value) in self.reader.metadata() {
            dict.set_item(key, py_from_value(value, py)?)?;
        }
        Ok(dict.into())
    }

    #[getter]
    pub fn get_parser(&self) -> PyResult<String> {
        Ok(self.parser.clone())
    }
}

#[pyproto]
impl PyObjectProtocol for Reader {
    fn __repr__(&self) -> PyResult<String> {
        Ok(format!("<Reader \"{}\">", self.parser))
    }
}

/// entab provides interconversion from streaming record formats.
#[pymodule]
fn entab(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<Reader>()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use pyo3::types::IntoPyDict;

    #[test]
    fn test_reader_creation() -> PyResult<()> {
        pyo3::prepare_freethreaded_python();
        let gil = Python::acquire_gil();
        let py = gil.python();

        // a filename or data has to be passed in
        assert!(Reader::new(None, None, None).is_err());

        // if data's passed in, it works
        let test_data = b">test\nACGT".to_object(py);
        let reader = Reader::new(Some(test_data.as_ref(py)), None, None)?;
        assert_eq!(&reader.parser, "fasta");

        // metadata are available
        let metadata = reader.get_metadata()?;
        assert!(metadata.as_ref(py).downcast::<PyDict>().is_ok());

        // headers are available
        let headers = reader.get_headers()?;
        assert_eq!(headers.len(), 2);

        Ok(())
    }

    #[test]
    fn test_reader_in_python() -> PyResult<()> {
        pyo3::prepare_freethreaded_python();
        let gil = Python::acquire_gil();
        let py = gil.python();

        let module = PyModule::new(py, "entab").unwrap();
        entab(py, module)?;
        let locals = [("entab", module)].into_py_dict(py);

        py.run(
            r#"
reader = entab.Reader(data=">test\nACGT")
assert reader.metadata == {}
for record in reader:
    pass
        "#,
            None,
            Some(locals),
        )?;

        Ok(())
    }
}
