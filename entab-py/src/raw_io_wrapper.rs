use std::io::{Error, Read};
use std::ptr::copy_nonoverlapping;

use pyo3::prelude::*;

pub struct RawIoWrapper {
    reader: Py<PyAny>,
}

impl RawIoWrapper {
    pub fn new(obj: &Bound<PyAny>) -> Self {
        let reader = obj.clone().unbind();
        RawIoWrapper { reader }
    }
}

impl Read for RawIoWrapper {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
        // TODO: it would be pass the buf itself into `readinto` so we're not
        // creating so many copies in here, but I can't figure out how to wrap
        // that into a python object that implements PyBufferProtocol properly
        Python::attach(|py| {
            let reader = self.reader.bind(py);
            let py_data = reader
                .call_method1("read", (buf.len(),))
                .map_err(|_| Error::other("`read` failed"))?;

            let amt_read = if let Ok(bytes) = py_data.extract::<Vec<u8>>() {
                unsafe {
                    copy_nonoverlapping::<u8>(bytes.as_ptr(), buf.as_mut_ptr(), bytes.len());
                }
                bytes.len()
            } else if let Ok(string) = py_data.extract::<String>() {
                let bytes = string.as_bytes();
                unsafe {
                    copy_nonoverlapping::<u8>(bytes.as_ptr(), buf.as_mut_ptr(), bytes.len());
                }
                bytes.len()
            } else {
                return Err(Error::other("`read` returned an unknown object"));
            };
            Ok(amt_read)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use pyo3::types::IntoPyDict;

    #[test]
    fn test_io_wrapper_bad_type() -> Result<(), Error> {
        Python::initialize();
        Python::attach(|py| {
            let mut scratch = Vec::new();

            let num = pyo3::types::PyFloat::new(py, 2.);
            let mut wrapper = RawIoWrapper::new(&num);
            assert!(wrapper.read_to_end(&mut scratch).is_err());
            Ok(())
        })
    }

    #[test]
    fn test_io_wrapper_stringio() -> Result<(), Error> {
        Python::initialize();
        Python::attach(|py| {
            let io_module = py.import("io")?;
            let locals = IntoPyDict::into_py_dict([("io", &io_module)], py)?;
            let mut scratch = Vec::new();

            let code = c"io.StringIO('>test\\nACGT')";
            let buffer: Bound<PyAny> = py.eval(code, None, Some(&locals))?;
            let mut wrapper = RawIoWrapper::new(&buffer);
            assert_eq!(wrapper.read_to_end(&mut scratch)?, 10);
            assert_eq!(scratch, b">test\nACGT");
            Ok(())
        })
    }

    #[test]
    fn test_io_wrapper_bytesio() -> Result<(), Error> {
        Python::initialize();
        Python::attach(|py| {
            let io_module = py.import("io")?;
            let locals = IntoPyDict::into_py_dict([("io", &io_module)], py)?;
            let mut scratch = Vec::new();

            let code = c"io.StringIO('>seq\\nTGCAT')";
            let buffer: Bound<PyAny> = py.eval(code, None, Some(&locals))?;
            let mut wrapper = RawIoWrapper::new(&buffer);
            assert_eq!(wrapper.read_to_end(&mut scratch)?, 10);
            assert_eq!(scratch, b">seq\nTGCAT");

            Ok(())
        })
    }
}
