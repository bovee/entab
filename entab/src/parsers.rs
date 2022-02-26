use alloc::format;
use core::any::type_name;
use core::convert::TryInto;
use core::marker::Copy;

use memchr::{memchr, memchr_iter};

use crate::EtError;

/// The default implementation is `impl<'r> FromSlice<'r> for ()` to simplify implementations for
/// e.g. state or other objects that don't read from the buffer.
pub trait FromSlice<'b>: Sized + Default {
    /// State is used to track information outside of the current slice scope that's used to create
    /// the value returned.
    type State;

    /// Given a slice and state, determine how much of the slice needs to be parsed to return a
    /// value and update `consumed` with that amount. If no value can be parsed, return Ok(false),
    /// otherwise return Ok(true) if a value can be parsed.
    ///
    /// # Errors
    /// If the parser fails or if there's not enough data in the buffer, an `EtError` will be returned.
    fn parse(
        _buffer: &[u8],
        _eof: bool,
        _consumed: &mut usize,
        _state: &mut Self::State,
    ) -> Result<bool, EtError> {
        Ok(true)
    }

    /// Given a slice and state, update Self by reading the information about the current record
    /// out.
    ///
    /// # Errors
    /// If buffer can not be interpreted into `Self`, return `EtError`.
    fn get(&mut self, _buffer: &'b [u8], _state: &Self::State) -> Result<(), EtError> {
        Ok(())
    }
}

/// Pull a `T` out of the slice, updating state appropriately and incrementing `consumed` to
/// account for bytes used.
///
/// # Errors
/// If an error extracting a value occured or if slice needs to be extended, return `EtError`.
#[inline]
pub fn extract<'r, T>(
    slice: &'r [u8],
    consumed: &mut usize,
    state: <T as FromSlice<'r>>::State,
) -> Result<T, EtError>
where
    T: FromSlice<'r> + Default + 'r,
{
    match extract_opt(slice, false, consumed, state)? {
        None => Err(format!(
            "Tried to extract {}, but parser indicated no more.",
            type_name::<T>()
        )
        .into()),
        Some(value) => Ok(value),
    }
}

/// Pull a `T` out of the slice, updating state appropriately and incrementing `consumed` to
/// account for bytes used.
///
/// # Errors
/// If an error extracting a value occured or if slice needs to be extended, return `EtError`.
#[inline]
pub fn extract_opt<'r, T>(
    slice: &'r [u8],
    eof: bool,
    consumed: &mut usize,
    mut state: <T as FromSlice<'r>>::State,
) -> Result<Option<T>, EtError>
where
    T: FromSlice<'r> + Default + 'r,
{
    let start = *consumed;
    if !T::parse(&slice[start..], eof, consumed, &mut state)? {
        return Ok(None);
    }
    let mut record = T::default();
    T::get(&mut record, &slice[start..*consumed], &state)?;
    Ok(Some(record))
}

/// Access long-lived fields in `Self::State` by bending the lifetime rules.
///
/// This should only be used for fields on a state object that are essentially immutable; if a
/// field is changed by successive `parse` calls, then this method will result in undefined
/// behavior and bads things could happen.
#[inline]
pub(crate) fn unsafe_access_state<'a, 'r, T>(state: &'a &'r mut T) -> &'r T {
    // this is equivalent to `*transmute::<&'a &'r mut T, &'a &'r T>(state)`
    let state_ptr: *const &mut T = state;
    unsafe { *state_ptr.cast::<&T>() }
}

/// The endianness of a number used to extract such a number.
#[derive(Clone, Copy, Debug)]
pub enum Endian {
    /// A number stored in big-endian format
    Big,
    /// A number stored in little-endian format
    Little,
}

impl Default for Endian {
    fn default() -> Self {
        Endian::Little
    }
}

macro_rules! impl_extract {
    ($return:ty) => {
        impl<'r> FromSlice<'r> for $return {
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

            fn get(&mut self, buf: &'r [u8], state: &Self::State) -> Result<(), EtError> {
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

impl<'r> FromSlice<'r> for () {
    type State = ();
}

impl<'r> FromSlice<'r> for &'r [u8] {
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
    fn get(&mut self, buf: &'r [u8], amt: &Self::State) -> Result<(), EtError> {
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
pub(crate) struct NewLine<'r>(pub(crate) &'r [u8]);

impl<'r> FromSlice<'r> for NewLine<'r> {
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
    fn get(&mut self, buf: &'r [u8], amt: &Self::State) -> Result<(), EtError> {
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

impl<'r> FromSlice<'r> for SeekPattern {
    // TODO: fix this lifetime to be more general?
    type State = &'static [u8];

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
    fn get(&mut self, _buf: &'r [u8], _amt: &Self::State) -> Result<(), EtError> {
        Ok(())
    }
}

/// Used to skip ahead in a buffer
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Skip;

impl<'r> FromSlice<'r> for Skip {
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
    fn get(&mut self, _buf: &'r [u8], _amt: &Self::State) -> Result<(), EtError> {
        Ok(())
    }
}
