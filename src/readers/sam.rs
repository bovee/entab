use std::mem;
use std::io::Write;

use memchr::memchr;

use crate::EtError;
use crate::metadata::StreamMetadata;
use crate::buffer::ReadBuffer;
use crate::record::{BindT, ReaderBuilder, Record, RecordReader};

pub struct SamRecordT;
impl<'b> BindT<'b> for SamRecordT {
    type Assoc = SamRecord<'b>;
}

pub struct SamRecord<'s> {
    query_name: &'s str,
    flag: u16,
    ref_name: &'s str,
    pos: u64,
    mapq: u8,
    cigar: &'s [u8],
    rnext: &'s str,
    pnext: u32,
    tlen: i32,
    seq: &'s [u8],
    qual: &'s [u8],
    extra: &'s str,
}

impl Record for SamRecord<'s> {
    fn size(&self) -> usize {
        12
    }

    fn write_field(&self, num: usize, writer: &mut dyn Write) -> Result<(), EtError> {
        // writer.write(self[num].as_bytes())?;
        Ok(())
    }
}


pub struct SamReaderBuilder;

impl Default for SamReaderBuilder {
    fn default() -> Self {
        SamReaderBuilder
    }
}

impl ReaderBuilder for SamReaderBuilder {
    type Item = SamRecordT;

    fn to_reader<'r>(&self, mut rb: ReadBuffer<'r>) -> Result<(Box<dyn RecordReader<Item=Self::Item> + 'r>, StreamMetadata), EtError> {
        // FIXME: parse headers
        let reader = SamReader<'r> {
            rb,
        };

        // return headers in metadata
        Ok((Box::new(reader), StreamMetadata::default()))
    }
}

pub struct SamReader<'r> {
    rb: ReadBuffer<'r>,
}

impl<'r> RecordReader for SamReader<'r> {
    type Item = SamRecordT;

    fn headers(&self) -> Vec<&str> {
        vec![]
    }

    fn next(&mut self) -> Result<Option<SamRecord>, EtError> {
        // TODO: need to skip line if it's a header?
        Ok(if let Some(line) = self.rb.read_line()? {
            let chunks: Vec<&[u8]> = line.split(b'\t').collect();
            if chunks.len() < 12 {
                return Err("Sam record too short".into());
            }
            Some(SamRecord {
                query_name: &'s str,
                flag: u16,
                ref_name: &'s str,
                pos: u64,
                mapq: u8,
                cigar: &'s [u8],
                rnext: &'s str,
                pnext: u32,
                tlen: i32,
                seq: &'s [u8],
                qual: &'s [u8],
                extra: &'s str,
            })
        } else {
            None
        })
    }
}
