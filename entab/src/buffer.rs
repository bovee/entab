use alloc::borrow::Cow;
#[cfg(feature = "std")]
use alloc::boxed::Box;
use core::convert::{AsRef, From, TryFrom};
#[cfg(feature = "std")]
use core::mem::swap;
#[cfg(feature = "std")]
use core::ptr;
#[cfg(feature = "std")]
use std::fs::File;
#[cfg(feature = "std")]
use std::io::{Cursor, Read};

use crate::parsers::FromSlice;
use crate::EtError;

/// Default buffer size
pub const BUFFER_SIZE: usize = 10_000;

/// Buffers Read to provide something that can be used for parsing
pub struct ReadBuffer<'r> {
    #[cfg(feature = "std")]
    reader: Box<dyn Read + 'r>,
    buffer: Cow<'r, [u8]>,
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
    /// Create a new buffer and associated ParserState.
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

    /// Refill the buffer from the reader and update the associated ParserState.
    ///
    /// If the buffer was successfully refilled return `true` and if the buffer could not be refilled
    /// (because it had previously reached EOF) return `false`.
    #[cfg(feature = "std")]
    pub fn refill(&mut self) -> Result<Option<&[u8]>, EtError> {
        if self.end {
            return Ok(None);
        } else if self.eof {
            self.end = true;
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
            .map_err(|e| EtError::from(e).add_context(self))?;
        buffer.truncate(len + amt_read);
        self.consumed = 0;
        swap(&mut Cow::Owned(buffer), &mut self.buffer);
        if amt_read == 0 {
            self.eof = true;
        }

        Ok(Some(&self.buffer[self.consumed..]))
    }

    /// Refill implementation for no_std
    #[cfg(not(feature = "std"))]
    pub fn refill(&mut self) -> Result<Option<&[u8]>, EtError> {
        if self.end {
            return Ok(None);
        } else if self.eof {
            self.end = true;
        }
        self.eof = true;
        Ok(Some(&self.buffer[self.consumed..]))
    }

    /// Uses the state to extract a record from the buffer
    #[inline]
    pub fn next<'n, T>(
        &'n mut self,
        mut state: <T as FromSlice<'n>>::State,
    ) -> Result<Option<T>, EtError>
    where
        T: FromSlice<'n> + 'n,
    {
        let mut consumed = self.consumed;
        loop {
            match T::parse(
                &self.buffer[consumed..],
                self.eof,
                &mut self.consumed,
                &mut state,
            ) {
                Ok(true) => {
                    self.record_pos += 1;
                    break;
                }
                Ok(false) => return Ok(None),
                Err(e) => {
                    if !e.incomplete || self.eof {
                        return Err(e.add_context(self));
                    }
                }
            }
            if self.refill()?.is_none() {
                return Ok(None);
            }
            consumed = 0;
        }
        let mut record = T::default();
        T::get(&mut record, &self.buffer[consumed..self.consumed], &state)
            .map_err(|e| e.add_context(self))?;
        Ok(Some(record))
    }

    /// Uses the state to extract a record from the buffer
    #[inline]
    pub fn next_no_refill<'n, T>(
        &'n mut self,
        mut state: <T as FromSlice<'n>>::State,
    ) -> Result<Option<T>, EtError>
    where
        T: FromSlice<'n> + 'n,
    {
        let consumed = self.consumed;
        match T::parse(
            &self.buffer[consumed..],
            self.eof,
            &mut self.consumed,
            &mut state,
        ) {
            Ok(true) => {
                self.record_pos += 1;
                let mut record = T::default();
                T::get(&mut record, &self.buffer[consumed..self.consumed], &state)
                    .map_err(|e| e.add_context(self))?;
                Ok(Some(record))
            }
            Ok(false) => Ok(None),
            Err(e) => {
                if !e.incomplete || self.eof {
                    Err(e.add_context(self))
                } else {
                    Ok(None)
                }
            }
        }
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
    //     #[cfg(feature = "std")]
    //     use alloc::boxed::Box;
    #[cfg(feature = "std")]
    use std::io::Cursor;

    use crate::parsers::{NewLine, SeekPattern};
    use crate::EtError;

    use super::ReadBuffer;
    //
    //     #[cfg(feature = "std")]
    //     #[test]
    //     fn test_buffer() -> Result<(), EtError> {
    //         let reader = Box::new(Cursor::new(b"123456"));
    //         let mut rb = ReadBuffer::new(reader)?;
    //
    //         assert_eq!(&rb[..], b"123456");
    //         let _ = rb.consume(3);
    //         assert_eq!(&rb[..], b"456");
    //         Ok(())
    //     }
    //
    //     #[cfg(feature = "std")]
    //     #[test]
    //     fn test_buffer_small() -> Result<(), EtError> {
    //         let reader = Box::new(Cursor::new(b"123456"));
    //         let mut rb = ReadBuffer::with_capacity(3, reader)?;
    //
    //         assert_eq!(&rb[..], b"123");
    //         assert_eq!(rb.consume(3), b"123");
    //         assert_eq!(&rb[..], b"");
    //
    //         rb.refill()?;
    //         assert_eq!(&rb[..], b"456");
    //         Ok(())
    //     }

    #[cfg(feature = "std")]
    #[test]
    fn test_read_lines() -> Result<(), EtError> {
        let mut rb = ReadBuffer::from_reader(Box::new(Cursor::new(b"1\n2\n3")), None)?;

        let mut ix = 0;
        while let Some(NewLine(line)) = rb.next(0)? {
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
        let _: Option<SeekPattern> = buffer.next(&b"1"[..])?;
        assert_eq!(&buffer.as_ref()[buffer.consumed..], b"1\n2\n3");
        let _: Option<SeekPattern> = buffer.next(&b"3"[..])?;
        assert_eq!(&buffer.as_ref()[buffer.consumed..], b"3");
        let e = buffer.next::<SeekPattern>(&b"1"[..])?;
        assert!(e.is_none());

        let mut buffer: ReadBuffer = b"ABC\n123\nEND"[..].into();
        assert_eq!(&buffer.as_ref()[buffer.consumed..], b"ABC\n123\nEND");
        let _: Option<SeekPattern> = buffer.next(&b"123"[..])?;
        assert_eq!(&buffer.as_ref()[buffer.consumed..], b"123\nEND");
        Ok(())
    }
    //
    //     #[cfg(feature = "std")]
    //     #[test]
    //     fn test_expansion() -> Result<(), EtError> {
    //         let reader = Box::new(Cursor::new(b"1234567890"));
    //         let mut rb = ReadBuffer::with_capacity(2, reader)?;
    //         assert!(rb.len() == 2);
    //         let _ = rb.refill();
    //         assert!(rb.len() >= 4);
    //         Ok(())
    //     }
}
