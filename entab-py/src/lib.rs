use std::fs::File;
use std::io::{Cursor, Error, ErrorKind, Read};
use std::ptr::copy_nonoverlapping;

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

struct RawIoWrapper {
    reader: PyObject,
}

impl RawIoWrapper {
    pub fn new(obj: &PyAny) -> Self {
        let gil = Python::acquire_gil();
        let py = gil.python();
        let reader = obj.to_object(py);
        RawIoWrapper { reader }
    }
}

impl Read for RawIoWrapper {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
        let gil = Python::acquire_gil();
        let py = gil.python();
        // TODO: it would be pass the buf itself into `readinto` so we're not
        // creating so many copies in here, but I can't figure out how to wrap
        // that into a python object that implements PyBufferProtocol properly
        let py_data = self
            .reader
            .call_method1(py, "read", (buf.len(),))
            .map_err(|_| {
                // TODO: get the error message from the python error?
                Error::new(ErrorKind::Other, "`read` failed")
            })?;

        let amt_read = if let Ok(bytes) = py_data.extract::<Vec<u8>>(py) {
            unsafe {
                copy_nonoverlapping::<u8>(bytes.as_ptr(), buf.as_mut_ptr(), bytes.len());
            }
            bytes.len()
        } else if let Ok(string) = py_data.extract::<String>(py) {
            let bytes = string.as_bytes();
            unsafe {
                copy_nonoverlapping::<u8>(bytes.as_ptr(), buf.as_mut_ptr(), bytes.len());
            }
            bytes.len()
        } else {
            return Err(Error::new(
                ErrorKind::Other,
                "`read` returned an unknown object",
            ));
        };
        Ok(amt_read)
    }
}

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
                } else if d.hasattr("read")? {
                    Box::new(RawIoWrapper::new(d))
                } else {
                    return Err(EntabError::py_err(
                        "`data` must be str, bytes or implement `read`",
                    ));
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
        let gil = Python::acquire_gil();
        let py = gil.python();

        // a filename or data has to be passed in
        assert!(Reader::new(None, None, None).is_err());

        // if data's passed in, it works
        let test_data = b">test\nACGT".to_object(py);
        let reader = Reader::new(Some(test_data.as_ref(py)), None, None)?;
        assert_eq!(&reader.parser, "fasta");
        Ok(())
    }
}
