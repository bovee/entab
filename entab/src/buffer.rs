use alloc::borrow::Cow;
#[cfg(feature = "std")]
use alloc::boxed::Box;
#[cfg(feature = "std")]
use core::convert::TryFrom;
use core::convert::{AsRef, From};
#[cfg(feature = "std")]
use core::mem::swap;
#[cfg(feature = "std")]
use core::ptr;
#[cfg(feature = "std")]
use std::fs::File;
#[cfg(feature = "std")]
use std::io::{Cursor, Read};

use crate::filetype::FileType;
use crate::parsers::FromSlice;
use crate::EtError;

/// Default buffer size
pub const BUFFER_SIZE: usize = 10_000;

/// Buffers Read to provide something that can be used for parsing
pub struct ReadBuffer<'r> {
    #[cfg(feature = "std")]
    reader: Box<dyn Read + 'r>,
    pub(crate) buffer: Cow<'r, [u8]>,
    /// The total amount of data read before byte 0 of this buffer (used for error messages)
    pub reader_pos: u64,
    /// The total number of records consumed (used for error messages)
    pub record_pos: u64,
    /// The amount of this buffer that's been marked as used
    pub consumed: usize,
    /// Is this the last chunk before EOF?
    pub eof: bool,
    /// After the parser has had a chance to run through eof, then this will be set to end parsing.
    pub end: bool,
}

impl<'r> ReadBuffer<'r> {
    /// Create a new buffer from a boxed `Read` trait.
    ///
    /// # Errors
    /// This will fail if there's an error reading into the buffer to initialize it.
    #[cfg(feature = "std")]
    pub fn from_reader(
        mut reader: Box<dyn Read + 'r>,
        buffer_size: Option<usize>,
    ) -> Result<Self, EtError> {
        let mut buffer = vec![0; buffer_size.unwrap_or(BUFFER_SIZE)];
        let amt_read = reader.read(&mut buffer)?;
        buffer.truncate(amt_read);
        Ok(ReadBuffer {
            reader,
            buffer: Cow::Owned(buffer),
            reader_pos: 0,
            record_pos: 0,
            consumed: 0,
            eof: false,
            end: false,
        })
    }

    /// Given a `ReadBuffer`, guess what kind of file it is.
    ///
    /// # Errors
    /// If an error reading data from the `reader` occurs, an error will be returned.
    pub fn sniff_filetype(&mut self) -> Result<FileType, EtError> {
        // try to get more if the buffer is *really* short
        if self.buffer.len() < 8 && !self.eof {
            let _ = self.refill()?;
        }
        Ok(FileType::from_magic(&self.buffer))
    }

    /// Refill the buffer from the reader.
    ///
    /// # Errors
    /// This will fail if there's an error retrieving data from the reader.
    #[cfg(feature = "std")]
    fn refill(&mut self) -> Result<bool, EtError> {
        if self.eof {
            return Ok(false);
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

        let len = buffer.len() - self.consumed;
        unsafe {
            // copy the old data to the front of the buffer
            let new_ptr = buffer.as_mut_ptr();
            let old_ptr = new_ptr.add(self.consumed);
            ptr::copy(old_ptr, new_ptr, len);

            // resize the buffer in prep to read in new data
            buffer.set_len(capacity);
        }
        let amt_read = self
            .reader
            .read(&mut buffer[len..])
            .map_err(|e| EtError::from(e).add_context_from_readbuffer(self))?;
        buffer.truncate(len + amt_read);
        self.consumed = 0;
        swap(&mut Cow::Owned(buffer), &mut self.buffer);
        if amt_read == 0 {
            self.eof = true;
        }

        Ok(true)
    }

    /// Refill implementation for no_std
    #[cfg(not(feature = "std"))]
    fn refill(&mut self) -> Result<bool, EtError> {
        if self.eof {
            return Ok(false);
        }
        self.eof = true;
        Ok(true)
    }

    /// Converts this `ReadBuffer` into a `Box<Read>`.
    #[cfg(feature = "std")]
    #[must_use]
    pub fn into_box_read(self) -> Box<dyn Read + 'r> {
        Box::new(Cursor::new(self.buffer).chain(self.reader))
    }

    /// Uses the state to extract a record from the buffer.
    ///
    /// # Errors
    /// Most commonly if the parser failed, but potentially also if the buffer could not be
    /// refilled.
    #[inline]
    pub fn next<'b: 's, 's, T>(
        &'b mut self,
        state: &'s mut <T as FromSlice<'b, 's>>::State,
    ) -> Result<Option<T>, EtError>
    where
        T: FromSlice<'b, 's>,
    {
        let mut consumed = self.consumed;
        loop {
            match T::parse(
                &self.buffer[consumed..],
                self.eof,
                &mut self.consumed,
                state,
            ) {
                Ok(true) => break,
                Ok(false) => return Ok(None),
                Err(e) => {
                    if !e.incomplete || self.eof {
                        return Err(e.add_context_from_readbuffer(self));
                    }
                    if !self.refill()? {
                        return Ok(None);
                    }
                    consumed = 0;
                }
            }
        }
        self.record_pos += 1;
        let mut record = T::default();
        T::get(&mut record, &self.buffer[consumed..self.consumed], state)
            .map_err(|e| e.add_context_from_readbuffer(self))?;
        Ok(Some(record))
    }

    /// Reads a record into an existing value.
    ///
    /// # Errors
    /// Errors for the same reasons as `next`.
    ///
    /// # Safety
    /// Don't use a previous record after calling this again.
    /// For example:
    /// ```ignore
    /// let x1: Record = Default::default();
    /// let x2: Record = Default::default();
    /// rb.next_into(&mut state, &mut x1)?;
    /// rb.next_into(&mut state, &mut x2)?;
    /// // x1 will now be in a bad state
    /// ```
    #[inline]
    #[doc(hidden)]
    pub unsafe fn next_into<'b: 's, 's, T>(
        &mut self,
        state: &mut <T as FromSlice<'b, 's>>::State,
        record: &mut T,
    ) -> Result<bool, EtError>
    where
        T: FromSlice<'b, 's>,
    {
        let mut consumed = self.consumed;
        loop {
            match T::parse(
                &self.buffer[consumed..],
                self.eof,
                &mut self.consumed,
                state,
            ) {
                Ok(true) => break,
                Ok(false) => return Ok(false),
                Err(e) => {
                    if !e.incomplete || self.eof {
                        return Err(e.add_context_from_readbuffer(self));
                    }
                    if !self.refill()? {
                        return Ok(false);
                    }
                    consumed = 0;
                }
            }
        }
        let buffer = { ::core::mem::transmute::<_, &'b Cow<'b, [u8]>>(&self.buffer) };
        let cur_state = {
            ::core::mem::transmute::<
                &mut <T as FromSlice<'b, 's>>::State,
                &'s mut <T as FromSlice<'b, 's>>::State,
            >(state)
        };
        self.record_pos += 1;
        T::get(record, &buffer[consumed..self.consumed], cur_state)
            .map_err(|e| e.add_context_from_readbuffer(self))?;
        Ok(true)
    }
}

impl<'r> ::core::fmt::Debug for ReadBuffer<'r> {
    fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
        write!(
            f,
            "<ReadBuffer pos={}:{} cur_len={} end={}>",
            self.record_pos,
            self.reader_pos + self.consumed as u64,
            self.as_ref().len(),
            self.eof,
        )
    }
}

#[cfg(feature = "std")]
impl<'r> TryFrom<Box<dyn Read + 'r>> for ReadBuffer<'r> {
    type Error = EtError;

    fn try_from(reader: Box<dyn Read + 'r>) -> Result<Self, Self::Error> {
        ReadBuffer::from_reader(reader, None)
    }
}

#[cfg(feature = "std")]
impl<'r> TryFrom<File> for ReadBuffer<'r> {
    type Error = EtError;

    fn try_from(reader: File) -> Result<Self, Self::Error> {
        ReadBuffer::from_reader(Box::new(reader), None)
    }
}

impl<'r> From<&'r [u8]> for ReadBuffer<'r> {
    fn from(buffer: &'r [u8]) -> Self {
        ReadBuffer {
            #[cfg(feature = "std")]
            reader: Box::new(Cursor::new(b"")),
            buffer: Cow::Borrowed(buffer),
            reader_pos: 0,
            record_pos: 0,
            consumed: 0,
            eof: true,
            end: false,
        }
    }
}

impl<'r> AsRef<[u8]> for ReadBuffer<'r> {
    fn as_ref(&self) -> &[u8] {
        &self.buffer
    }
}

#[cfg(test)]
mod test {
    #[cfg(feature = "std")]
    use alloc::boxed::Box;
    #[cfg(feature = "std")]
    use std::io::Cursor;

    use crate::parsers::common::{NewLine, SeekPattern};
    use crate::EtError;

    use super::ReadBuffer;

    #[cfg(feature = "std")]
    #[test]
    fn test_buffer_small() -> Result<(), EtError> {
        let reader = Box::new(Cursor::new(b"123456"));
        let rb = ReadBuffer::from_reader(reader, None)?;
        // the default buffer size should always be above 6 or something's gone really wrong
        assert_eq!(&rb.as_ref()[rb.consumed..], b"123456");

        let reader = Box::new(Cursor::new(b"123456"));
        let mut rb = ReadBuffer::from_reader(reader, Some(3))?;

        assert_eq!(&rb.as_ref()[rb.consumed..], b"123");
        rb.consumed += 3;
        assert!(rb.refill()?);
        assert_eq!(&rb.as_ref()[rb.consumed..], b"456");
        Ok(())
    }

    #[test]
    fn test_read_lines() -> Result<(), EtError> {
        let mut rb = ReadBuffer::from(&b"1\n2\n3"[..]);

        let mut ix = 0;
        while let Some(NewLine(line)) = rb.next(&mut 0)? {
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

    // FIXME: delete this and add tests in parsers.rs?
    // #[test]
    // fn test_read_lines_from_slice() -> Result<(), EtError> {
    //     let rb = &b"1\n2\n3"[..];
    //     let mut ix = 0;
    //     while let NewLine(line) = extract(rb, &mut 0, 0)? {
    //         match ix {
    //             0 => assert_eq!(line, b"1"),
    //             1 => assert_eq!(line, b"2"),
    //             2 => assert_eq!(line, b"3"),
    //             _ => panic!("Invalid index; buffer tried to read too far"),
    //         }
    //         ix += 1;
    //     }
    //     assert_eq!(ix, 3);
    //     Ok(())
    // }

    #[test]
    fn test_seek_pattern() -> Result<(), EtError> {
        let mut buffer: ReadBuffer = b"1\n2\n3"[..].into();
        let _: Option<SeekPattern> = buffer.next(&mut &b"1"[..])?;
        assert_eq!(&buffer.as_ref()[buffer.consumed..], b"1\n2\n3");
        let _: Option<SeekPattern> = buffer.next(&mut &b"3"[..])?;
        assert_eq!(&buffer.as_ref()[buffer.consumed..], b"3");
        let e = buffer.next::<SeekPattern>(&mut &b"1"[..])?;
        assert!(e.is_none());

        let mut buffer: ReadBuffer = b"ABC\n123\nEND"[..].into();
        assert_eq!(&buffer.as_ref()[buffer.consumed..], b"ABC\n123\nEND");
        let _: Option<SeekPattern> = buffer.next(&mut &b"123"[..])?;
        assert_eq!(&buffer.as_ref()[buffer.consumed..], b"123\nEND");
        Ok(())
    }

    #[cfg(feature = "std")]
    #[test]
    fn test_expansion() -> Result<(), EtError> {
        let reader = Box::new(Cursor::new(b"1234567890"));
        let mut rb = ReadBuffer::from_reader(reader, Some(2))?;
        assert!(rb.as_ref().len() == 2);
        let _ = rb.refill();
        assert!(rb.as_ref().len() >= 4);
        Ok(())
    }

    #[test]
    fn test_next_into() -> Result<(), EtError> {
        let mut rb = ReadBuffer::from(&b"1\n2\n3"[..]);

        let mut ix = 0;
        let mut line: NewLine = Default::default();
        while unsafe { rb.next_into(&mut 0, &mut line)? } {
            match ix {
                0 => assert_eq!(&line.0, b"1"),
                1 => assert_eq!(&line.0, b"2"),
                2 => assert_eq!(&line.0, b"3"),
                _ => panic!("Invalid index; buffer tried to read too far"),
            }
            ix += 1;
        }
        assert_eq!(ix, 3);
        Ok(())
    }
}
