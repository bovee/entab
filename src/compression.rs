use std::io::{Cursor, Error, Read};

use bzip2::read::BzDecoder;
use flate2::read::MultiGzDecoder;
use xz2::read::XzDecoder;

use crate::BUFFER_SIZE;
use crate::mime::FileType;


pub fn decompress<'a>(mut reader: Box<dyn Read + 'a>) -> Result<(Box<dyn Read + 'a>, FileType), Error> {
    let mut first = vec![0; BUFFER_SIZE];
    let amt_read = reader.read(&mut first)?;
    unsafe {
        first.set_len(amt_read);
    }

    let file_type = FileType::from_magic(&first);
    let cursor = Cursor::new(first);
    match file_type {
        FileType::Gzip => {
            let gz_reader = MultiGzDecoder::new(cursor.chain(reader));
            decompress(Box::new(gz_reader))
        },
        FileType::Bzip => {
            let bz_reader = BzDecoder::new(cursor.chain(reader));
            decompress(Box::new(bz_reader))
        },
        FileType::Lzma => {
            let xz_reader = XzDecoder::new(cursor.chain(reader));
            decompress(Box::new(xz_reader))
        },
        x => Ok((Box::new(cursor.chain(reader)), x)),
    }
}
