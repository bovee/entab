use alloc::borrow::Cow;
use memchr::{memchr, memchr_iter};

/// Replace all tab characters in `buf` with `replace_char`
pub fn replace_tabs(buf: &[u8], replace_char: u8) -> Cow<[u8]> {
    // first part is a fast check to see if we need to do any allocations
    let first;
    match memchr(b'\t', &buf) {
        Some(break_loc) => first = break_loc,
        None => return buf.into(),
    }

    let mut new_buf = buf.to_vec();
    new_buf[first] = replace_char;
    for pos in memchr_iter(b'\t', &buf[first..]) {
        new_buf[pos] = replace_char;
    }
    new_buf.into()
}

#[test]
fn test_replace_tabs() {
    assert_eq!(&replace_tabs(b"", b'|')[..], b"");
    assert_eq!(&replace_tabs(b"\t", b'|')[..], b"|");
    assert_eq!(&replace_tabs(b"test", b'|')[..], b"test");
    assert_eq!(&replace_tabs(b"\ttest", b'|')[..], b"|test");
    assert_eq!(&replace_tabs(b"\ttest\t", b'|')[..], b"|test|");
}
