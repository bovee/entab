use alloc::format;
use core::convert::TryInto;
use core::marker::Copy;

use memchr::{memchr, memchr_iter};

use crate::error::EtError;
use crate::parsers::{Endian, FromSlice};

macro_rules! impl_extract {
    ($return:ty) => {
        impl<'b: 's, 's> FromSlice<'b, 's> for $return {
            type State = Endian;

            #[inline]
            fn parse(
                buf: &[u8],
                _eof: bool,
                consumed: &mut usize,
                _state: &mut Self::State,
            ) -> Result<bool, EtError> {
                if buf.len() < core::mem::size_of::<$return>() {
                    let err: EtError =
                        format!("Could not read {}", ::core::any::type_name::<$return>()).into();
                    return Err(err.incomplete());
                }
                *consumed += core::mem::size_of::<$return>();
                Ok(true)
            }

            fn get(&mut self, buf: &'b [u8], state: &Self::State) -> Result<(), EtError> {
                let slice = buf[..core::mem::size_of::<$return>()].try_into().unwrap();
                *self = match state {
                    Endian::Big => <$return>::from_be_bytes(slice),
                    Endian::Little => <$return>::from_le_bytes(slice),
                };
                Ok(())
            }
        }
    };
}

impl_extract!(i8);
impl_extract!(u8);
impl_extract!(i16);
impl_extract!(u16);
impl_extract!(i32);
impl_extract!(u32);
impl_extract!(i64);
impl_extract!(u64);
impl_extract!(f32);
impl_extract!(f64);

impl<'b: 's, 's> FromSlice<'b, 's> for () {
    type State = ();
}

impl<'b: 's, 's> FromSlice<'b, 's> for &'b [u8] {
    type State = usize;

    #[inline]
    fn parse(
        buf: &[u8],
        _eof: bool,
        consumed: &mut usize,
        amt: &mut Self::State,
    ) -> Result<bool, EtError> {
        if buf.len() < *amt {
            let err: EtError = format!("Could not extract a slice of size {}", amt).into();
            return Err(err.incomplete());
        }
        *consumed += *amt;
        Ok(true)
    }

    #[inline]
    fn get(&mut self, buf: &'b [u8], amt: &Self::State) -> Result<(), EtError> {
        *self = &buf[..*amt];
        Ok(())
    }
}

/// Used to read a single line out of the buffer.
///
/// Assumes all lines are terminated with a '\n' and an optional '\r'
/// before so should handle almost all current text file formats, but
/// may fail on older '\r' only formats.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct NewLine<'b>(pub(crate) &'b [u8]);

impl<'b: 's, 's> FromSlice<'b, 's> for NewLine<'b> {
    type State = usize;

    #[inline]
    fn parse(
        buf: &[u8],
        eof: bool,
        consumed: &mut usize,
        state: &mut Self::State,
    ) -> Result<bool, EtError> {
        if buf.is_empty() {
            if eof {
                return Ok(false);
            }
            return Err(EtError::new("Could not extract a new line").incomplete());
        }
        // find the newline
        let (end, to_consume) = if let Some(e) = memchr(b'\n', buf) {
            if buf[..e].last() == Some(&b'\r') {
                (e - 1, e + 1)
            } else {
                (e, e + 1)
            }
        } else if eof {
            // we couldn't find a new line, but we are at the end of the file
            // so return everything to the EOF
            let l = buf.len();
            (l, l)
        } else {
            // couldn't find the character; load more
            return Err(EtError::new("Could not extract a new line").incomplete());
        };
        *state = end;

        *consumed += to_consume;
        Ok(true)
    }

    #[inline]
    fn get(&mut self, buf: &'b [u8], amt: &Self::State) -> Result<(), EtError> {
        self.0 = &buf[..*amt];
        Ok(())
    }
}

/// Used to read from a buffer until the given `state` slice is found and then discard everything before
/// that `state` slice. Note that this never returns a consumed length of more than 0 because it
/// silently updates the state as it consumes so it doesn't have to re-search the buffer if the
/// buffer needs to be refilled.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct SeekPattern;

impl<'b: 's, 's> FromSlice<'b, 's> for SeekPattern {
    type State = &'s [u8];

    #[inline]
    fn parse(
        buffer: &[u8],
        eof: bool,
        consumed: &mut usize,
        pat: &mut Self::State,
    ) -> Result<bool, EtError> {
        for pos in memchr_iter(pat[0], buffer) {
            if pos + pat.len() > buffer.len() {
                *consumed += pos;
                if eof {
                    return Ok(false);
                }
                let err: EtError = format!(
                    "{:?} may be at end of buffer, but no more could be pulled",
                    pat
                )
                .into();
                return Err(err.incomplete());
            }
            if &buffer[pos..pos + pat.len()] == *pat {
                *consumed += pos;
                return Ok(true);
            }
        }

        *consumed = buffer.len();
        if eof {
            return Ok(false);
        }
        let err: EtError = format!("Could not find {:?}", pat).into();
        Err(err.incomplete())
    }

    #[inline]
    fn get(&mut self, _buf: &'b [u8], _amt: &Self::State) -> Result<(), EtError> {
        Ok(())
    }
}

/// Used to skip ahead in a buffer
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Skip;

impl<'b: 's, 's> FromSlice<'b, 's> for Skip {
    type State = usize;

    #[inline]
    fn parse(
        buffer: &[u8],
        _eof: bool,
        consumed: &mut usize,
        amt: &mut Self::State,
    ) -> Result<bool, EtError> {
        if buffer.len() < *consumed + *amt {
            *consumed += buffer.len();
            let err: EtError =
                format!("Buffer terminated before {} bytes could be skipped.", amt).into();
            return Err(err.incomplete());
        }
        *consumed += *amt;
        Ok(true)
    }

    #[inline]
    fn get(&mut self, _buf: &'b [u8], _amt: &Self::State) -> Result<(), EtError> {
        Ok(())
    }
}
