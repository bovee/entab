use std::io::{self, BufRead, Read};
use std::ptr;

use crate::BUFFER_SIZE;


/// Wraps a Box<Read> to allow buffered reading
///
/// Primary differences from Rust's built-in BufReader:
///  - residual in buffer is maintained between `fill_buf`s
///  - buffer will be expanded if not enough data present to parse
pub struct ReadBuffer<'s> {
    buffer: Vec<u8>,
    reader: Box<dyn Read + 's>,
    start: usize,
}


impl<'s> ReadBuffer<'s> {
    pub fn new(reader: Box<dyn Read + 's>) -> Result<Self, io::Error> {
        Self::with_capacity(BUFFER_SIZE, reader)
    }

    pub fn with_capacity(buffer_size: usize, mut reader: Box<dyn Read + 's>) -> Result<Self, io::Error> {
        let mut buffer = Vec::with_capacity(buffer_size);
        unsafe { buffer.set_len(buffer.capacity()); }
        let amt_read = reader.read(&mut buffer)?;
        unsafe { buffer.set_len(amt_read); }

        Ok(ReadBuffer {
            buffer,
            reader,
            start: 0,
        })
    }
}

impl<'s> AsRef<[u8]> for ReadBuffer<'s> {
    fn as_ref(&self) -> &[u8] {
        &self.buffer[self.start..]
    }
}

impl<'s> Read for ReadBuffer<'s> {
     fn read(&mut self, buf: &mut [u8]) -> Result<usize, io::Error> {
         let n = Read::read(&mut self.fill_buf()?, buf)?;
         self.consume(n);
         Ok(n)
     }
}

impl<'s> BufRead for ReadBuffer<'s> {
    fn consume(&mut self, amt: usize) {
        self.start += amt;
    }

    fn fill_buf(&mut self) -> Result<&[u8], io::Error> {
        let mut capacity = self.buffer.capacity();
        if self.start == 0 {
            self.buffer.reserve(2 * capacity);
            capacity = self.buffer.capacity();
        };

        let len = self.buffer.len() - self.start;
        unsafe {
            let new_ptr = self.buffer.as_mut_ptr();
            let old_ptr = new_ptr.add(self.start);
            ptr::copy(old_ptr, new_ptr, len);
        }

        unsafe { self.buffer.set_len(capacity); }
        let amt_read = self.reader.read(&mut self.buffer[len..])?;
        unsafe { self.buffer.set_len(len + amt_read); }
        self.start = 0;

        Ok(&self.buffer)
    }
}


#[cfg(test)]
mod test {
    use std::io::{self, BufRead, Cursor};
    
    use super::ReadBuffer;

    #[test]
    fn test_buffer() -> Result<(), io::Error> {
        let reader = Box::new(Cursor::new(b"123456"));
        let mut rb = ReadBuffer::new(reader)?;

        assert_eq!(rb.as_ref(), b"123456");
        rb.consume(3);
        assert_eq!(rb.as_ref(), b"456");
        Ok(())
    }


    #[test]
    fn test_buffer_small() -> Result<(), io::Error> {
        let reader = Box::new(Cursor::new(b"123456"));
        let mut rb = ReadBuffer::with_capacity(3, reader)?;

        assert_eq!(rb.as_ref(), b"123");
        rb.consume(3);
        assert_eq!(rb.as_ref(), b"");

        let new_read = rb.fill_buf()?;
        assert_eq!(&new_read, b"456");
        assert_eq!(rb.as_ref(), b"456");
        Ok(())
    }

    #[ignore]
    #[test]
    fn test_rust_built_in() -> Result<(), io::Error> {
        use std::io::BufReader;

        let reader = Box::new(Cursor::new(b"123456"));
        let mut rb = BufReader::with_capacity(3, reader);

        assert_eq!(rb.buffer(), b"");

        let new_read = rb.fill_buf()?;
        assert_eq!(&new_read, b"123");
        assert_eq!(rb.buffer(), b"123");

        // nothing consumed so the buffer doesn't expand
        let new_read = rb.fill_buf()?;
        assert_eq!(&new_read, b"123");
        assert_eq!(rb.buffer(), b"123");

        // anything still in the buffer; it doesn't expand
        rb.consume(2);
        let new_read = rb.fill_buf()?;
        assert_eq!(&new_read, b"3");
        assert_eq!(rb.buffer(), b"3");

        // everything consumed; buffer can pull in the next data
        rb.consume(1);
        let new_read = rb.fill_buf()?;
        assert_eq!(&new_read, b"456");
        assert_eq!(rb.buffer(), b"456");
        Ok(())
    }
}
