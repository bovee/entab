use alloc::borrow::Cow;
#[cfg(feature = "std")]
use alloc::boxed::Box;
use alloc::format;
#[cfg(feature = "std")]
use alloc::vec::Vec;
use core::any::type_name;
#[cfg(feature = "std")]
use core::mem::swap;
use core::ops::{Index, Range, RangeFrom, RangeFull, RangeTo};
#[cfg(feature = "std")]
use core::ptr;
#[cfg(feature = "std")]
use std::io::{Cursor, Read};

use memchr::memchr;

use crate::parsers::FromBuffer;
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
    pub buffer: Cow<'s, [u8]>,
    /// The stream to read from
    #[cfg(feature = "std")]
    reader: Box<dyn Read + 's>,
    /// The total amount of data read before byte 0 of this buffer (used for error messages)
    pub reader_pos: u64,
    /// The total number of records consumed (used for error messages)
    pub record_pos: u64,
    /// The amount of this buffer that's been marked as used
    pub consumed: usize,
    /// Is this the last chunk before EOF?
    pub eof: bool,
}

impl<'s> ReadBuffer<'s> {
    /// Create a new ReadBuffer from the `reader` using the default size.
    #[cfg(feature = "std")]
    pub fn new(reader: Box<dyn Read + 's>) -> Result<Self, EtError> {
        Self::with_capacity(BUFFER_SIZE, reader)
    }

    /// Create a new ReadBuffer from the `reader` using the size provided
    #[cfg(feature = "std")]
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
        // it's possible amt_read < buffer.capacity() for e.g. reading out of
        // a compressed stream where different chunks can be smaller than the
        // buffer length so we can't infer anything about EOF from amt_read.

        Ok(ReadBuffer {
            buffer: Cow::Owned(buffer),
            reader,
            reader_pos: 0,
            record_pos: 0,
            consumed: 0,
            eof: false,
        })
    }

    /// Create a new ReadBuffer from `slice`
    ///
    /// Buffer will automatically be at "eof" and `refill` will have no
    /// effect.
    pub fn from_slice(slice: &'s [u8]) -> Self {
        ReadBuffer {
            buffer: Cow::Borrowed(slice),
            #[cfg(feature = "std")]
            reader: Box::new(Cursor::new(b"")),
            reader_pos: 0,
            record_pos: 0,
            consumed: 0,
            eof: true,
        }
    }

    /// Refill the buffer from the `reader`; if no data has been consumed the
    /// buffer's size if doubled and the new buffer is filled.
    #[cfg(feature = "std")]
    pub fn refill(&mut self) -> Result<(), EtError> {
        if self.eof {
            return Ok(());
        }

        // pull the buffer out; if self.buffer's Borrowed then eof should
        // always be true above and we shouldn't hit this
        let mut tmp_buffer = Cow::Borrowed(&b""[..]);
        swap(&mut self.buffer, &mut tmp_buffer);
        let mut buffer = tmp_buffer.into_owned();

        // track how much data was in the reader before the data in the buffer
        self.reader_pos += self.consumed as u64;

        let mut capacity = buffer.capacity();
        // if we haven't read anything, but we want more data expand the buffer
        if self.consumed == 0 {
            buffer.reserve(2 * capacity);
            capacity = buffer.capacity();
        };

        // copy the old data to the front of the buffer
        let len = buffer.len() - self.consumed;
        unsafe {
            let new_ptr = buffer.as_mut_ptr();
            let old_ptr = new_ptr.add(self.consumed);
            ptr::copy(old_ptr, new_ptr, len);
        }

        // resize the buffer and read in new data
        unsafe {
            buffer.set_len(capacity);
        }
        let amt_read = self
            .reader
            .read(&mut buffer[len..])
            .map_err(|e| EtError::from(e).add_context(&self))?;
        unsafe {
            buffer.set_len(len + amt_read);
        }
        self.consumed = 0;
        swap(&mut Cow::Owned(buffer), &mut self.buffer);
        if amt_read == 0 {
            self.eof = true;
        }

        Ok(())
    }

    /// Refill the buffer; since `no_std` doesn't support the Read trait, this
    /// is a noop.
    #[cfg(not(feature = "std"))]
    pub fn refill(&mut self) -> Result<(), EtError> {
        // no_std doesn't support Readers so this is always just an
        // unrefillable slice
        return Ok(());
    }

    /// Same result as `refill`, but ensures the buffer is at least `amt` bytes
    /// large. Will error if not enough data is available.
    pub fn reserve(&mut self, amt: usize) -> Result<(), EtError> {
        while self.len() < amt {
            if self.eof {
                return Err(EtError::new("Data ended prematurely", &self));
            }
            self.refill()?;
        }
        Ok(())
    }

    /// Move the buffer to the start of the first found location of `pat`.
    /// If `pat` is not found, the buffer will be exhausted.
    pub fn seek_pattern(&mut self, pat: &[u8]) -> Result<bool, EtError> {
        loop {
            if let Some(pos) = memchr(pat[0], &self[..]) {
                if pos + pat.len() > self.len() {
                    if self.eof() {
                        let _ = self.consume(self.len());
                        return Ok(false);
                    }
                    let _ = self.consume(pos);
                    self.refill()?;
                    continue;
                }
                if &self[pos..pos + pat.len()] == pat {
                    let _ = self.consume(pos);
                    break;
                }
                let _ = self.consume(1);
                continue;
            } else if self.eof() {
                let _ = self.consume(self.len());
                return Ok(false);
            }
            // couldn't find the character; load more
            if self.len() > pat.len() - 1 {
                let _ = self.consume(self.len() + 1 - pat.len());
            }
            self.refill()?;
        }
        Ok(true)
    }

    /// Returns the byte slice of size requested and marks that data as used
    /// so the next time `refill`/`reserve`/`extract` are called, this memory
    /// can be freed.
    pub fn consume(&mut self, amt: usize) -> &[u8] {
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

    /// The byte position that the reader is on
    pub fn get_byte_pos(&self) -> u64 {
        self.reader_pos + self.consumed as u64
    }

    /// Get a record of type `T` from this ReadBuffer, taking a `state`
    /// whose type is dependent on `T` and consuming the bytes used to
    /// represent that record.
    ///
    /// Please note that each call to this function may resize the
    /// underlying buffer and invalidate any previous references so you should
    /// manually implement parsers that will retrieve multiple referential
    /// records (e.g. calling extract::<&[u8]>() multiple times will not work).
    #[inline]
    pub fn extract<'r, T>(&'r mut self, state: T::State) -> Result<T, EtError>
    where
        T: FromBuffer<'r> + Default,
    {
        if let Some(record) = T::get(self, state)? {
            return Ok(record);
        }
        // TODO: it would be nice to use EtError::new here
        Err(EtError::from(format!(
            "Could not get {} from stream",
            type_name::<Self>()
        )))
    }
}

impl<'r> ::core::fmt::Debug for ReadBuffer<'r> {
    fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
        write!(
            f,
            "<ReadBuffer pos={}:{} cur_len={} end={}>",
            self.record_pos,
            self.reader_pos + self.consumed as u64,
            self.len(),
            self.eof()
        )
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
    #[cfg(feature = "std")]
    use alloc::boxed::Box;
    #[cfg(feature = "std")]
    use std::io::Cursor;

    use crate::parsers::{FromBuffer, NewLine};
    use crate::EtError;

    use super::ReadBuffer;

    #[cfg(feature = "std")]
    #[test]
    fn test_buffer() -> Result<(), EtError> {
        let reader = Box::new(Cursor::new(b"123456"));
        let mut rb = ReadBuffer::new(reader)?;

        assert_eq!(&rb[..], b"123456");
        let _ = rb.consume(3);
        assert_eq!(&rb[..], b"456");
        Ok(())
    }

    #[cfg(feature = "std")]
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

    #[cfg(feature = "std")]
    #[test]
    fn test_read_lines() -> Result<(), EtError> {
        let reader = Box::new(Cursor::new(b"1\n2\n3"));
        let mut rb = ReadBuffer::with_capacity(3, reader)?;

        let mut ix = 0;
        while let Some(NewLine(line)) = NewLine::get(&mut rb, ())? {
            match ix {
                0 => assert_eq!(line, b"1"),
                1 => assert_eq!(line, b"2"),
                2 => assert_eq!(line, b"3"),
                _ => panic!("Invalid index; buffer tried to read too far"),
            }
            ix += 1;
        }
        assert_eq!(ix, 3);
        Ok(())
    }

    #[test]
    fn test_read_lines_from_slice() -> Result<(), EtError> {
        let mut rb = ReadBuffer::from_slice(b"1\n2\n3");
        let mut ix = 0;
        while let Some(NewLine(line)) = NewLine::get(&mut rb, ())? {
            match ix {
                0 => assert_eq!(line, b"1"),
                1 => assert_eq!(line, b"2"),
                2 => assert_eq!(line, b"3"),
                _ => panic!("Invalid index; buffer tried to read too far"),
            }
            ix += 1;
        }
        assert_eq!(ix, 3);
        Ok(())
    }

    #[test]
    fn test_seek_pattern() -> Result<(), EtError> {
        let mut rb = ReadBuffer::from_slice(b"1\n2\n3");
        assert_eq!(rb.seek_pattern(b"1")?, true);
        assert_eq!(&rb[..], b"1\n2\n3");
        assert_eq!(rb.seek_pattern(b"3")?, true);
        assert_eq!(&rb[..], b"3");
        assert_eq!(rb.seek_pattern(b"1")?, false);

        let mut rb = ReadBuffer::from_slice(b"ABC\n123\nEND");
        assert_eq!(rb.seek_pattern(b"123")?, true);
        assert_eq!(&rb[..], b"123\nEND");
        Ok(())
    }

    #[cfg(feature = "std")]
    #[test]
    fn test_expansion() -> Result<(), EtError> {
        let reader = Box::new(Cursor::new(b"1234567890"));
        let mut rb = ReadBuffer::with_capacity(2, reader)?;
        assert!(rb.len() == 2);
        let _ = rb.refill();
        assert!(rb.len() >= 4);
        Ok(())
    }
}
