use std::fs::File;
use std::io::{Cursor, Read};

use entab_base::buffer::ReadBuffer;
use entab_base::compression::decompress;
use entab_base::readers::{get_builder, RecordReader};
use entab_base::record::Record;
use entab_base::utils::error::EtError;
use pyo3::class::PyIterProtocol;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyTuple};
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
        let mut record_class = slf.record_class.clone_ref(py);
        let rec = if let Some(val) = slf.reader.next().map_err(to_py)? {
            if record_class.as_ref(py).is_none() {
                let headers: Vec<String> = val.headers().iter().map(|s| s.to_string()).collect();
                let collections = PyModule::import(py, "collections")?;
                record_class = collections.call1("namedtuple", ("Record", headers))?.into();
            };
            match val {
                Record::Mz {
                    time,
                    mz,
                    intensity,
                } => record_class.as_ref(py).call1((time, mz, intensity))?,
                Record::MzFloat {
                    time,
                    mz,
                    intensity,
                } => record_class.as_ref(py).call1((time, mz, intensity))?,
                Record::Sam {
                    query_name,
                    flag,
                    ref_name,
                    pos,
                    mapq,
                    cigar,
                    rnext,
                    pnext,
                    tlen,
                    seq,
                    qual,
                    extra,
                } => {
                    let tup = PyTuple::new(
                        py,
                        &[
                            query_name.to_string().to_object(py),
                            flag.to_object(py),
                            ref_name.to_string().to_object(py),
                            pos.to_object(py),
                            mapq.to_object(py),
                            PyBytes::new(py, &cigar).into(),
                            rnext.to_string().to_object(py),
                            pnext.to_object(py),
                            tlen.to_object(py),
                            PyBytes::new(py, &seq).into(),
                            PyBytes::new(py, &qual).into(),
                            PyBytes::new(py, &extra).into(),
                        ],
                    );
                    // call1 doesn't support tuples bigger than size 10 so we
                    // have to coerce it into a PyTuple first
                    record_class.as_ref(py).call1(tup)?
                }
                Record::Sequence {
                    id,
                    sequence,
                    quality,
                } => record_class.as_ref(py).call1((
                    id.to_string(),
                    PyBytes::new(py, &sequence),
                    quality.map(|x| PyBytes::new(py, x)),
                ))?,
                Record::Tsv(rec, _) => record_class.as_ref(py).call1(PyTuple::new(py, rec))?,
            }
        } else {
            return Ok(None);
        };
        slf.record_class = record_class.clone_ref(py);
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
        let reader = if let Some(builder) = get_builder(parser_name) {
            builder.to_reader(buffer).map_err(to_py)?
        } else {
            return Err(EntabError::py_err(
                "No parser could be found for the data provided",
            ));
        };
        let gil = Python::acquire_gil();
        let py = gil.python();
        Ok(Reader {
            parser: parser_name.to_string(),
            record_class: py.None().as_ref(py).into(),
            reader,
        })
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
