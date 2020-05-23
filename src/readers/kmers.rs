use alloc::borrow::Cow;
use alloc::boxed::Box;
use alloc::format;
use alloc::vec;
use alloc::vec::Vec;
use core::mem::transmute;

use memchr::memchr;

use crate::buffer::ReadBuffer;
use crate::record::{BindT, ReaderBuilder, Record, RecordReader};
use crate::EtError;
use crate::utils::string::replace_tabs;


#[derive(Debug)]
pub struct KmerRecord<'s> {
    pub id: Cow<'s, str>,
    pub kmer_pos: usize,
    pub kmer: Cow<'s, [u8]>,
}

impl<'s> Record for KmerRecord<'s> {
    fn size(&self) -> usize {
        3
    }

    fn write_field<W>(&self, index: usize, mut write: W) -> Result<(), EtError>
    where
        W: FnMut(&[u8]) -> Result<(), EtError>,
    {
        match index {
            0 => write(self.id.as_bytes())?,
            1 => write(&format!("{}", &self.kmer_pos).as_bytes())?,
            2 => write(&self.kmer)?,
            _ => panic!("FASTQ field index out of range"),
        };
        Ok(())
    }
}

pub struct KmerRecordT;
impl<'b> BindT<'b> for KmerRecordT {
    type Assoc = KmerRecord<'b>;
}

pub struct FastaKmerReaderBuilder {
    // TODO: add a skip N's?
    // TODO: add a remove newlines? (default true)
    k: u8,
}

impl Default for FastaKmerReaderBuilder {
    fn default() -> Self {
        FastaKmerReaderBuilder { k: 21 }
    }
}

impl ReaderBuilder for FastaKmerReaderBuilder {
    type Item = KmerRecordT;

    fn to_reader<'r>(
        &self,
        rb: ReadBuffer<'r>,
    ) -> Result<Box<dyn RecordReader<Item = Self::Item> + 'r>, EtError> {
        Ok(Box::new(FastaKmerReader {
            rb,
            id: None,
            seq_pos: 0,
            k: self.k as usize,
        }))
    }
}

pub struct FastaKmerReader<'r> {
    rb: ReadBuffer<'r>,
    id: Option<String>,
    seq_pos: usize,
    k: usize,
}

impl<'r> RecordReader for FastaKmerReader<'r> {
    type Item = KmerRecordT;

    fn headers(&self) -> Vec<&str> {
        vec!["id", "sequence"]
    }

    fn next(&mut self) -> Result<Option<KmerRecord>, EtError> {
        if self.id.is_none() {

        }
        
        // if (sequence too short for k?) {
        // TODO: read the next header/id and save in self.header
        // } else {
        // }
        if self.rb.eof() && self.rb.len() < self.k {
            return Ok(None);
        }

        // let (seq_range, rec_end) = loop {
        //     let (header_end, seq_start) = if let Some(p) = memchr(b'\n', &self.rb[..]) {
        //         (p, p + 1)
        //     } else if self.rb.eof() {
        // }

        Ok(Some(KmerRecord {
            id: Cow::Borrowed(&self.id.as_ref().unwrap()),
            kmer_pos: 0, // FIXME
            kmer: Cow::Borrowed(&self.rb[self.seq_pos..self.seq_pos + self.k]),
        }))
    }
}

pub struct FastqKmerReaderBuilder {
    // TODO: add a quality mask?
    k: u8,
}

impl Default for FastqKmerReaderBuilder {
    fn default() -> Self {
        FastqKmerReaderBuilder { k: 21 }
    }
}

impl ReaderBuilder for FastqKmerReaderBuilder {
    type Item = KmerRecordT;

    fn to_reader<'r>(
        &self,
        rb: ReadBuffer<'r>,
    ) -> Result<
        Box<dyn RecordReader<Item = Self::Item> + 'r>,
        EtError,
    > {
        let fastq_reader = FastqReader { rb };
        Ok(
            Box::new(FastqKmerReader {
                fastq_reader,
                k: self.k as usize,
                id: "",
                kmer_pos: 0,
                sequence: b"",
            })
        )
    }
}

pub struct FastqKmerReader<'r> {
    fastq_reader: FastqReader<'r>,
    k: usize,
    id: &'r str,
    kmer_pos: usize,
    // TODO: make this Cow and optionally strip_returns?
    sequence: &'r [u8],
}

impl<'r> RecordReader for FastqKmerReader<'r> {
    type Item = KmerRecordT;

    fn headers(&self) -> Vec<&str> {
        vec!["id", "sequence"]
    }

    fn next(&mut self) -> Result<Option<KmerRecord>, EtError> {
        if !self.sequence.is_empty() {
            self.sequence = &self.sequence[1..];
            self.kmer_pos += 1;
        }
        if self.sequence.len() < self.k {
            self.id = "";
            self.sequence = b"";
            while let Some(record) = self.fastq_reader.next()? {
                if record.sequence.len() < self.k {
                    continue;
                }
                // we need to do a lifetime trick here; these
                // records should be safe as long as we're not
                // changing the underlying fastq_reader, but the
                // compiler doesn't know we're not doing that
                unsafe {
                    self.id = transmute(record.id);
                    self.sequence = transmute(record.sequence);
                }
                self.kmer_pos = 0;
                break;
            }
            if self.sequence.len() < self.k {
                // we never found a good sequence
                return Ok(None);
            }
        }

        Ok(Some(KmerRecord {
            id: self.id.into(),
            kmer_pos: self.kmer_pos,
            kmer: self.sequence[..self.k].into(),
        }))
    }
}
