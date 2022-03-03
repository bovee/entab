use alloc::format;
use core::convert::TryInto;
#[cfg(feature = "std")]
use std::sync::Mutex;

use crate::buffer::ReadBuffer;
use crate::error::EtError;
use crate::parsers::FromSlice;

/// A chunk of a ReadBuffer that's safe to pass among threads
#[derive(Clone, Copy, Debug)]
#[doc(hidden)]
#[cfg(feature = "std")]
pub struct BufferChunk {
    /// The amount currently consumed by parsing
    pub consumed: usize,
    /// If this is the last chunk to parse
    pub eof: bool,
    /// The total amount of data read before byte 0 of this buffer (used for error messages)
    pub reader_pos: u64,
    /// The total number of records consumed (used for error messages)
    pub record_pos: u64,
}

#[cfg(feature = "std")]
impl BufferChunk {
    /// Create a new buffer chunk
    pub fn new(consumed: usize, eof: bool, reader_pos: u64, record_pos: u64) -> Self {
        BufferChunk {
            consumed,
            eof,
            reader_pos,
            record_pos,
        }
    }

    /// Read the next record out of the chunk.
    pub fn next<'n, T>(
        &mut self,
        buffer: &'n [u8],
        mut_state: &Mutex<<T as FromSlice<'n>>::State>,
    ) -> Result<Option<T>, EtError>
    where
        T: FromSlice<'n>,
    {
        // TODO: better error handling
        let mut state = mut_state.lock().unwrap();
        let consumed = self.consumed;
        match T::parse(
            &buffer[consumed..],
            self.eof,
            &mut self.consumed,
            &mut state,
        ) {
            Ok(true) => {},
            Ok(false) => return Ok(None),
            Err(e) => {
                if !e.incomplete || self.eof {
                    return Err(e.add_context(buffer, self.consumed, self.record_pos, self.reader_pos));
                } else {
                    return Ok(None);
                }
            }
        }
        self.record_pos += 1;
        let mut record = T::default();
        T::get(&mut record, &buffer[consumed..self.consumed], &state)
            .map_err(|e| e.add_context(buffer, self.consumed, self.record_pos, self.reader_pos))?;
        // TODO: update `consumed` and `record_pos` here instead
        Ok(Some(record))
    }
}

#[cfg(feature = "std")]
unsafe impl Send for BufferChunk {}
#[cfg(feature = "std")]
unsafe impl Sync for BufferChunk {}

/// Set up a state and a `ReadBuffer` for parsing.
#[doc(hidden)]
pub fn init_state<'r, S, B, P>(data: B, params: Option<P>) -> Result<(ReadBuffer<'r>, S), EtError>
where
    B: TryInto<ReadBuffer<'r>>,
    EtError: From<<B as TryInto<ReadBuffer<'r>>>::Error>,
    S: for<'a> FromSlice<'a, State = P>,
    P: Default,
{
    let mut buffer = data.try_into()?;
    if let Some(state) = buffer.next::<S>(params.unwrap_or_default())? {
        Ok((buffer, state))
    } else {
        Err(format!(
            "Could not initialize state {}",
            ::core::any::type_name::<S>()
        )
        .into())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[cfg(feature = "std")]
    use std::fs::File;
    #[cfg(feature = "std")]
    use std::sync::Arc;
    #[cfg(feature = "std")]
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[cfg(feature = "std")]
    use rayon;

    use crate::readers::fastq::{FastqRecord, FastqState};
        
    #[test]
    fn test_chunked_read() -> Result<(), EtError> {
        let f: &[u8] = include_bytes!("../tests/data/test.fastq");
        let (mut rb, mut state) = init_state::<FastqState, _, _>(f, None).unwrap();
        let mut seq_len = 0;
        while let Some(FastqRecord { sequence, ..}) = rb.next(&mut state)? {
            seq_len += sequence.len();
        }
        assert_eq!(seq_len, 250000);
        Ok(())
    }

    #[cfg(feature = "std")]
    #[test]
    fn test_multithreaded_read() -> Result<(), EtError> {
        let f = File::open("./tests/data/test.fastq")?;
        let (mut rb, mut state) = init_state::<FastqState, _, _>(f, None)?;
        let seq_len = Arc::new(AtomicUsize::new(0));
        while let Some((slice, mut chunk)) = rb.next_chunk()? {
            let mut_state = Mutex::new(&mut state);
            let chunk = rayon::scope(|s| {
                while let Some(FastqRecord { sequence, ..}) = chunk.next(slice, &mut_state).map_err(|e| e.to_string())? {
                    let sl = seq_len.clone();
                    s.spawn(move |_| {
                        let _ = sl.fetch_add(sequence.len(), Ordering::Relaxed);
                    });
                }
                Ok::<_, String>(chunk)
            })?;
            rb.update_from_chunk(chunk);
        }
        assert_eq!(seq_len.load(Ordering::Relaxed), 250000);

        Ok(())
    }
}

