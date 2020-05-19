use alloc::boxed::Box;
use alloc::vec::Vec;
use core::ops::{Index, Range, RangeFrom, RangeFull, RangeTo};
use core::ptr;
#[cfg(std)]
use std::fs::File;
use std::io::Read;

use memchr::memchr;

use crate::EtError;

/// Default buffer size
pub const BUFFER_SIZE: usize = 1000;

/// Wraps a Box<Read> to allow buffered reading
///
/// Primary differences from Rust's built-in BufReader:
///  - residual in buffer is maintained between `fill_buf`s
///  - buffer will be expanded if not enough data present to parse
///  - EOF state is tracked
pub struct ReadBuffer<'s> {
    /// The primary buffer; reloaded from `reader` when needed
    pub buffer: Vec<u8>,
    /// The stream to read from
    reader: Box<dyn Read + 's>,
    /// The total amount of data read before byte 0 of this buffer (used for error messages)
    reader_pos: u64,
    /// The total number of records consumed (used for error messages)
    record_pos: u64,
    /// The amount of this buffer that's been marked as used
    pub consumed: usize,
    /// Is this the last chunk before EOF?
    pub eof: bool,
}

impl<'s> ReadBuffer<'s> {
    /// Create a new ReadBuffer from the `reader` using the default size.
    pub fn new(reader: Box<dyn Read + 's>) -> Result<Self, EtError> {
        Self::with_capacity(BUFFER_SIZE, reader)
    }

    /// Create a new ReadBuffer from the `reader` using the size provided
    pub fn with_capacity(
        buffer_size: usize,
        mut reader: Box<dyn Read + 's>,
    ) -> Result<Self, EtError> {
        let mut buffer = Vec::with_capacity(buffer_size);
        unsafe {
            buffer.set_len(buffer.capacity());
        }
        let amt_read = reader.read(&mut buffer)?;
        unsafe {
            buffer.set_len(amt_read);
        }
        let eof = amt_read != buffer.capacity();

        Ok(ReadBuffer {
            buffer,
            reader,
            reader_pos: 0,
            record_pos: 0,
            consumed: 0,
            eof,
        })
    }

    /// Create a new new ReadBuffer from the `file`
    #[cfg(std)]
    pub fn from_file(file: &'s File) -> Result<Self, EtError> {
        Self::new(Box::new(file))
    }

    /// Refill the buffer from the `reader`; if no data has been consumed the
    /// buffer's size if doubled and the new buffer is filled.
    pub fn refill(&mut self) -> Result<(), EtError> {
        if self.eof {
            return Ok(());
        }
        let mut capacity = self.buffer.capacity();
        // if we haven't read anything, but we want more data expand the buffer
        if self.consumed == 0 {
            self.buffer.reserve(2 * capacity);
            capacity = self.buffer.capacity();
        };

        // copy the old data to the front of the buffer
        let len = self.buffer.len() - self.consumed;
        unsafe {
            let new_ptr = self.buffer.as_mut_ptr();
            let old_ptr = new_ptr.add(self.consumed);
            ptr::copy(old_ptr, new_ptr, len);
        }

        // resize the buffer and read in new data
        unsafe {
            self.buffer.set_len(capacity);
        }
        let amt_read = self
            .reader
            .read(&mut self.buffer[len..])
            .map_err(|e| EtError::from(e).fill_pos(&self))?;
        unsafe {
            self.buffer.set_len(len + amt_read);
        }
        self.consumed = 0;
        if amt_read != capacity - len {
            self.eof = true;
        }

        // track how much data was in the reader before the data in the buffer
        self.reader_pos += len as u64;
        Ok(())
    }

    /// Mark out the data in the buffer and return a reference to it
    /// To be called once an entire record has been consumed
    pub fn consume(&mut self, amt: usize) -> &[u8] {
        self.record_pos += 1;
        self.partial_consume(amt)
    }

    pub fn partial_consume(&mut self, amt: usize) -> &[u8] {
        let start = self.consumed;
        self.consumed += amt;
        &self.buffer[start..self.consumed]
    }

    /// True if this is the last chunk in the stream
    pub fn eof(&self) -> bool {
        self.eof
    }

    /// True if any data is left in the buffer
    pub fn is_empty(&self) -> bool {
        self.consumed >= self.buffer.len()
    }

    /// How much data is in the buffer
    pub fn len(&self) -> usize {
        self.buffer.len() - self.consumed
    }

    /// The record and byte position that the reader is on
    pub fn get_pos(&self) -> (u64, u64) {
        (self.record_pos, self.reader_pos + self.consumed as u64)
    }

    /// Read a single line out of the buffer.
    ///
    /// Assumes all lines are terminated with a '\n' and an optional '\r'
    /// before so should handle almost all current text file formats, but
    /// may fail on older '\r' only formats.
    pub fn read_line(&mut self) -> Result<Option<&[u8]>, EtError> {
        if self.is_empty() {
            return Ok(None);
        }
        // find the newline
        let (end, to_consume) = loop {
            if let Some(e) = memchr(b'\n', &self[..]) {
                if self[..e].last() == Some(&b'\r') {
                    break (e - 1, e + 1);
                } else {
                    break (e, e + 1);
                }
            } else if self.eof() {
                // we couldn't find a new line, but we are at the end of the file
                // so return everything to the EOF
                let l = self.len();
                break (l, l);
            }
            // couldn't find the character; load more
            self.refill()?;
        };

        let buffer = self.consume(to_consume);
        Ok(Some(&buffer[..end]))
    }
}

// It's not really possible to implement Index<(Bound, Bound)> or otherwise
// make this generic over all forms of Range* so we do a little hacky business
macro_rules! impl_index {
    ($index:ty, $return:ty) => {
        impl<'r> Index<$index> for ReadBuffer<'r> {
            type Output = $return;

            fn index(&self, index: $index) -> &Self::Output {
                &self.buffer[self.consumed..][index]
            }
        }
    };
}

impl_index!(Range<usize>, [u8]);
impl_index!(RangeFrom<usize>, [u8]);
impl_index!(RangeTo<usize>, [u8]);
impl_index!(RangeFull, [u8]);
impl_index!(usize, u8);

#[cfg(test)]
mod test {
    use alloc::boxed::Box;
    use std::io::Cursor;

    use crate::EtError;

    use super::ReadBuffer;

    #[test]
    fn test_buffer() -> Result<(), EtError> {
        let reader = Box::new(Cursor::new(b"123456"));
        let mut rb = ReadBuffer::new(reader)?;

        assert_eq!(&rb[..], b"123456");
        rb.consume(3);
        assert_eq!(&rb[..], b"456");
        Ok(())
    }

    #[test]
    fn test_buffer_small() -> Result<(), EtError> {
        let reader = Box::new(Cursor::new(b"123456"));
        let mut rb = ReadBuffer::with_capacity(3, reader)?;

        assert_eq!(&rb[..], b"123");
        assert_eq!(rb.consume(3), b"123");
        assert_eq!(&rb[..], b"");

        rb.refill()?;
        assert_eq!(&rb[..], b"456");
        Ok(())
    }

    #[test]
    fn test_read_lines() -> Result<(), EtError> {
        let reader = Box::new(Cursor::new(b"1\n2\n3"));
        let mut rb = ReadBuffer::with_capacity(3, reader)?;

        let mut ix = 0;
        while let Some(l) = rb.read_line()? {
            match ix {
                0 => assert_eq!(l, b"1"),
                1 => assert_eq!(l, b"2"),
                2 => assert_eq!(l, b"3"),
                _ => panic!("Invalid index; buffer tried to read too far"),
            }
            ix += 1;
        }
        assert_eq!(ix, 3);
        Ok(())
    }
}
