use alloc::borrow::Cow;
use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;
use core::mem::transmute;

use crate::buffer::ReadBuffer;
use crate::readers::fastq::FastqReader;
use crate::record::{ReaderBuilder, Record, RecordReader};
use crate::EtError;

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
    fn to_reader<'r>(&self, rb: ReadBuffer<'r>) -> Result<Box<dyn RecordReader + 'r>, EtError> {
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
    fn headers(&self) -> Vec<&str> {
        vec!["id", "sequence"]
    }

    fn next(&mut self) -> Result<Option<Record>, EtError> {
        if self.id.is_none() {}

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

        Ok(Some(Record::Kmer {
            id: Cow::Borrowed(&self.id.as_ref().unwrap()),
            kmer: Cow::Borrowed(&self.rb[self.seq_pos..self.seq_pos + self.k]),
            sequence_index: 0, // FIXME
            kmer_index: 0,     // FIXME
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
    fn to_reader<'r>(&self, rb: ReadBuffer<'r>) -> Result<Box<dyn RecordReader + 'r>, EtError> {
        let fastq_reader = FastqReader { rb };
        Ok(Box::new(FastqKmerReader {
            fastq_reader,
            k: self.k as usize,
            id: "",
            kmer_pos: 0,
            sequence: b"",
        }))
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
    fn headers(&self) -> Vec<&str> {
        vec!["id", "sequence"]
    }

    fn next(&mut self) -> Result<Option<Record>, EtError> {
        if !self.sequence.is_empty() {
            self.sequence = &self.sequence[1..];
            self.kmer_pos += 1;
        }
        if self.sequence.len() < self.k {
            self.id = "";
            self.sequence = b"";
            while let Some(Record::Fastq { id, sequence, .. }) = self.fastq_reader.next()? {
                if sequence.len() < self.k {
                    continue;
                }
                // we need to do a lifetime trick here; these
                // records should be safe as long as we're not
                // changing the underlying fastq_reader, but the
                // compiler doesn't know we're not doing that
                unsafe {
                    self.id = transmute(id);
                    self.sequence = transmute(sequence);
                }
                self.kmer_pos = 0;
                break;
            }
            if self.sequence.len() < self.k {
                // we never found a good sequence
                return Ok(None);
            }
        }

        Ok(Some(Record::Kmer {
            id: self.id.into(),
            kmer: self.sequence[..self.k].into(),
            sequence_index: 0, // FIXME
            kmer_index: 0,     // FIXME
        }))
    }
}
