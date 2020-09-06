use alloc::format;
use core::convert::TryInto;
use core::marker::Copy;

use memchr::memchr;

use crate::buffer::ReadBuffer;
use crate::EtError;

pub trait FromBuffer<'r>: Sized {
    type State;

    fn get(rb: &'r mut ReadBuffer, state: Self::State) -> Result<Self, EtError>;
}

pub(crate) trait FromSlice<'r>: Sized {
    type State;

    fn out_of(rb: &'r [u8], state: Self::State) -> Result<Self, EtError>;
}

#[derive(Clone, Copy, Debug)]
pub enum Endian {
    Big,
    Little,
}

macro_rules! impl_extract {
    ($return:ty) => {
        impl<'r> FromBuffer<'r> for $return {
            type State = Endian;

            #[inline]
            fn get(rb: &'r mut ReadBuffer, state: Self::State) -> Result<Self, EtError> {
                rb.reserve(core::mem::size_of::<$return>())?;
                let slice = rb
                    .consume(core::mem::size_of::<$return>())
                    .try_into()
                    .unwrap();

                Ok(match state {
                    Endian::Big => <$return>::from_be_bytes(slice),
                    Endian::Little => <$return>::from_le_bytes(slice),
                })
            }
        }

        impl<'r> FromSlice<'r> for $return {
            type State = Endian;

            #[inline]
            fn out_of(rb: &'r [u8], state: Self::State) -> Result<Self, EtError> {
                if rb.len() < core::mem::size_of::<$return>() {
                    return Err(EtError::new(format!(
                        "Could not read {}",
                        core::any::type_name::<$return>()
                    )));
                }
                let slice = rb[..core::mem::size_of::<$return>()].try_into().unwrap();
                Ok(match state {
                    Endian::Big => <$return>::from_be_bytes(slice),
                    Endian::Little => <$return>::from_le_bytes(slice),
                })
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

impl<'r> FromBuffer<'r> for () {
    type State = ();

    #[inline]
    fn get(_rb: &'r mut ReadBuffer, _amt: Self::State) -> Result<Self, EtError> {
        Ok(())
    }
}

impl<'r> FromBuffer<'r> for &'r [u8] {
    type State = usize;

    #[inline]
    fn get(rb: &'r mut ReadBuffer, amt: Self::State) -> Result<Self, EtError> {
        rb.reserve(amt)?;
        Ok(rb.consume(amt))
    }
}

/// Used to read a single line out of the buffer.
///
/// Assumes all lines are terminated with a '\n' and an optional '\r'
/// before so should handle almost all current text file formats, but
/// may fail on older '\r' only formats.
#[derive(Clone, Copy, Debug)]
pub struct NewLine<'r>(pub &'r [u8]);

impl<'r> FromBuffer<'r> for Option<NewLine<'r>> {
    type State = ();

    #[inline]
    fn get(rb: &'r mut ReadBuffer, _state: Self::State) -> Result<Self, EtError> {
        if rb.is_empty() {
            return Ok(None);
        }
        // find the newline
        let (end, to_consume) = loop {
            if let Some(e) = memchr(b'\n', &rb[..]) {
                if rb[..e].last() == Some(&b'\r') {
                    break (e - 1, e + 1);
                } else {
                    break (e, e + 1);
                }
            } else if rb.eof() {
                // we couldn't find a new line, but we are at the end of the file
                // so return everything to the EOF
                let l = rb.len();
                break (l, l);
            }
            // couldn't find the character; load more
            rb.refill()?;
        };

        let buffer = rb.extract::<&[u8]>(to_consume)?;
        Ok(Some(NewLine(&buffer[..end])))
    }
}
