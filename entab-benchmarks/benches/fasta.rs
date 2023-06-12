use std::str::from_utf8;

use memchr::{memchr, memchr_iter};


pub fn read_fasta<F>(
    mut rb: &[u8],
    mut callback: F,
    ) -> Result<(), &str>
where
F: for<'a> FnMut(&'a str, &[u8]) -> (),
{
    let mut id;
    let mut raw_sequence;
    let mut new_buf = Vec::with_capacity(1024);
    while !rb.is_empty() {
        if rb[0] != b'>' {
            // TODO: check for spaces at the very end?
            return Err("Valid FASTA records start with '>'");
        }
        let seq_start = if let Some(p) = memchr(b'\n', rb) {
            if p > 0 && rb[p - 1] == b'\r' {
                // strip out the \r too if this is a \r\n ending
                id = from_utf8(&rb[1..p - 1]).map_err(|_| "bad utf8 in header")?;
            } else {
                id = from_utf8(&rb[1..p]).map_err(|_| "bad utf8 in header")?;
            }
            p + 1
        } else {
            return Err("Incomplete header");
        };

        if let Some(p) = memchr(b'>', &rb[seq_start..]) {
            if p == 0 || rb.get(seq_start + p - 1) != Some(&b'\n') {
                return Err("Unexpected '>' found");
            }
            if rb.get(seq_start + p - 2) == Some(&b'\r') {
                raw_sequence = &rb[seq_start..seq_start + p - 2];
            } else {
                raw_sequence = &rb[seq_start..seq_start + p - 1];
            }
            rb = &rb[seq_start + p..];
        } else {
            raw_sequence = &rb[seq_start..rb.len()];
            // at eof; just return the end
            rb = b"";
        }

        let mut seq_newlines = memchr_iter(b'\n', raw_sequence).peekable();
        let sequence = if seq_newlines.peek().is_none() {
            raw_sequence.as_ref()
        } else {
            let mut start = 0;
            new_buf.clear();
            for pos in seq_newlines {
                if pos >= 1 && raw_sequence.get(pos - 1) == Some(&b'\r') {
                    new_buf.extend_from_slice(&raw_sequence[start..pos - 1]);
                } else {
                    new_buf.extend_from_slice(&raw_sequence[start..pos]);
                }
                start = pos + 1;
            }
            new_buf.extend_from_slice(&raw_sequence[start..]);
            new_buf.as_ref()
        };
        callback(id, sequence);
    }
    Ok(())
}
