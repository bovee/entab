use std::io::{Error, ErrorKind, Read};
use std::ptr::copy_nonoverlapping;

use pyo3::prelude::*;

pub struct RawIoWrapper {
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
