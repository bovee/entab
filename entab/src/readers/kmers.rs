use alloc::borrow::Cow;
use alloc::string::String;
use core::mem::transmute;

use crate::buffer::ReadBuffer;
use crate::readers::fastq::FastqReader;
use crate::readers::RecordReader;
use crate::record::Record;
use crate::EtError;

pub struct FastaKmerReader<'r> {
    rb: ReadBuffer<'r>,
    id: Option<String>,
    seq_pos: usize,
    k: usize,
}

impl<'r> FastaKmerReader<'r> {
    pub fn new(rb: ReadBuffer<'r>, k: u8) -> Result<Self, EtError> {
        // TODO: add a skip N's?
        // TODO: add a remove newlines? (default true)
        Ok(FastaKmerReader {
            rb,
            id: None,
            seq_pos: 0,
            k: k as usize,
        })
    }
}

impl<'r> RecordReader for FastaKmerReader<'r> {
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

        Ok(Some(Record::Sequence {
            id: &self.id.as_ref().unwrap(),
            sequence: Cow::Borrowed(&self.rb[self.seq_pos..self.seq_pos + self.k]),
            quality: None,
        }))
    }
}

pub struct FastqKmerReader<'r> {
    fastq_reader: FastqReader<'r>,
    k: usize,
    id: &'r str,
    kmer_pos: usize,
    sequence: &'r [u8],
}

impl<'r> FastqKmerReader<'r> {
    pub fn new(rb: ReadBuffer<'r>, k: u8) -> Result<Self, EtError> {
        // TODO: add a quality mask?
        let fastq_reader = FastqReader { rb };
        Ok(FastqKmerReader {
            fastq_reader,
            k: k as usize,
            id: "",
            kmer_pos: 0,
            sequence: b"",
        })
    }
}

impl<'r> RecordReader for FastqKmerReader<'r> {
    fn next(&mut self) -> Result<Option<Record>, EtError> {
        if !self.sequence.is_empty() {
            self.sequence = &self.sequence[1..];
            self.kmer_pos += 1;
        }
        if self.sequence.len() < self.k {
            self.id = "";
            self.sequence = b"";
            while let Some(Record::Sequence { id, sequence, .. }) = self.fastq_reader.next()? {
                if sequence.len() < self.k {
                    continue;
                }
                // we need to do a lifetime trick here; these
                // records should be safe as long as we're not
                // changing the underlying fastq_reader, but the
                // compiler doesn't know we're not doing that
                unsafe {
                    self.id = transmute(id);
                    self.sequence = transmute(sequence.as_ref());
                }
                self.kmer_pos = 0;
                break;
            }
            if self.sequence.len() < self.k {
                // we never found a good sequence
                return Ok(None);
            }
        }

        Ok(Some(Record::Sequence {
            id: self.id,
            sequence: self.sequence[..self.k].into(),
            quality: None,
        }))
    }
}
