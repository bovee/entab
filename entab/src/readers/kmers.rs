use core::mem::transmute;

use crate::buffer::ReadBuffer;
use crate::readers::fastq::{FastqReader, FastqRecord};
use crate::readers::RecordReader;
use crate::record::Value;
use crate::EtError;

// TODO: impl FastaKmerReader; rather than wrapping FastaReader, it should
// allow parsing e.g. super-large contig genomes without rebuffering (in a
// constant-sized buffer) by iterating over the underlying file directly
// TODO: add a skip N's?
// TODO: add a remove newlines? (default true)

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
        let fastq_reader = FastqReader::new(rb, ())?;
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
    fn next_record(&mut self) -> Result<Option<Vec<Value>>, EtError> {
        if !self.sequence.is_empty() {
            self.sequence = &self.sequence[1..];
            self.kmer_pos += 1;
        }
        if self.sequence.len() < self.k {
            self.id = "";
            self.sequence = b"";
            while let Some(FastqRecord { id, sequence, .. }) = self.fastq_reader.next()? {
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

        Ok(Some(vec![self.id.into(), self.sequence[..self.k].into()]))
    }

    fn headers(&self) -> Vec<String> {
        vec!["id".to_string(), "sequence".to_string()]
    }
}
