use std::borrow::Cow;
use memchr::{memchr, memchr_iter};

/// Replace all `replace_char` bytes in `buf` with `replace_with`.
pub fn replace_chars<'a>(buf: &'a [u8], replace_char: u8, replace_with: &[u8]) -> Cow<'a, [u8]> {
    // first part is a fast check to see if we need to do any allocations
    let first;
    match memchr(replace_char, &buf) {
        Some(break_loc) => first = break_loc,
        None => return buf.into(),
    }

    if replace_with.len() == 1 {
        let mut new_buf = buf.to_vec();
        new_buf[first] = replace_with[0];
        for pos in memchr_iter(replace_char, &buf[first..]) {
            new_buf[pos] = replace_with[0];
        }
        new_buf.into()
    } else {
        let mut new_buf = Vec::with_capacity(buf.len());
        new_buf.extend(&buf[..first]);
        new_buf.extend(replace_with);
        let old_pos = first + 1;
        for pos in memchr_iter(replace_char, &buf[first..]) {
            new_buf.extend(&buf[old_pos..pos]);
            new_buf.extend(replace_with);
            old_pos = pos + 1;
        }
        if old_pos < buf.len() {
            new_buf.extend(&buf[old_pos + 1..]);
        }
        new_buf.into()
    }
}
