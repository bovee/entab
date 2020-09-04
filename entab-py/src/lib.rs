use std::fs::File;
use std::io::{Cursor, Read};

use entab_base::buffer::ReadBuffer;
use entab_base::compression::decompress;
use entab_base::readers::{get_reader, RecordReader};
use entab_base::record::Value;
use entab_base::utils::error::EtError;
use pyo3::class::{PyIterProtocol, PyObjectProtocol};
use pyo3::prelude::*;
use pyo3::types::PyTuple;
use pyo3::{create_exception, exceptions};

create_exception!(entab, EntabError, exceptions::Exception);

fn to_py(err: EtError) -> PyErr {
    EntabError::py_err(err.to_string())
}

// TODO: remove the unsendable; by wrapping reader in an Arc?
#[pyclass(unsendable)]
#[text_signature = "(/, data=None, filename=None, parser=None)"]
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
        let val: PyObject = FromPy::from_py(slf, py);
        Ok(val.clone_ref(py))
    }

    fn __next__(mut slf: PyRefMut<Self>) -> PyResult<Option<Py<PyAny>>> {
        let gil = Python::acquire_gil();
        let py = gil.python();
        let rec = if let Some(val) = slf.reader.next_record().map_err(to_py)? {
            let mut data = Vec::with_capacity(val.len());
            for field in val {
                data.push(match field {
                    Value::Null => py.None().as_ref(py).into(),
                    Value::Boolean(b) => b.to_object(py),
                    Value::Datetime(d) => d.to_object(py),
                    Value::Float(v) => v.to_object(py),
                    Value::Integer(v) => v.to_object(py),
                    Value::String(s) => s.to_object(py),
                    _ => {
                        return Err(EntabError::py_err(
                            "record and list subelements unimplemented",
                        ))
                    }
                })
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
        let stream: Box<dyn Read> = match (data, filename) {
            (Some(d), None) => {
                if let Ok(bytes) = d.extract::<Vec<u8>>() {
                    Box::new(Cursor::new(bytes))
                } else if let Ok(string) = d.extract::<String>() {
                    Box::new(Cursor::new(string.into_bytes()))
                } else {
                    // TODO: handle `read()` objects (RawIOBase)?
                    // maybe we could impl std::io::Read on those and
                    // pass through the `mut &[u8]` Read calls for to the
                    // `readinto` function with another adaptor?
                    return Err(EntabError::py_err("`data` must be a str or bytes"));
                }
            }
            (None, Some(f)) => Box::new(File::open(f)?),
            _ => {
                return Err(EntabError::py_err(
                    "One and only one of `data` or `filename` must be provided",
                ))
            }
        };
        let (reader, filetype, _) = decompress(stream).map_err(to_py)?;
        let buffer = ReadBuffer::new(reader).map_err(to_py)?;

        let parser_name = parser.unwrap_or_else(|| filetype.to_parser_name());
        let reader = get_reader(parser_name, buffer).map_err(to_py)?;
        let gil = Python::acquire_gil();
        let py = gil.python();

        let headers: Vec<String> = reader.headers();
        let collections = PyModule::import(py, "collections")?;
        let record_class = collections.call1("namedtuple", ("Record", headers))?.into();

        Ok(Reader {
            parser: parser_name.to_string(),
            record_class,
            reader,
        })
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

    #[test]
    fn test_reader_creation() -> PyResult<()> {
        // a filename or data has to be passed in
        assert!(Reader::new(None, None, None).is_err());

        Ok(())
    }
}
