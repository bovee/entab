use std::io::{Error, Read};

#[cfg(feature = "compression")]
use bzip2::read::BzDecoder;
#[cfg(feature = "compression")]
use flate2::read::MultiGzDecoder;
#[cfg(feature = "compression")]
use xz2::read::XzDecoder;
#[cfg(feature = "compression")]
use zstd::stream::read::Decoder as ZstdDecoder;

use crate::filetype::{sniff_reader_filetype, FileType};

/// Decompress a `Read` stream and returns the inferred file type
/// (compiled without decompression support so decompression won't occur)
#[cfg(not(feature = "compression"))]
pub fn decompress<'a>(
    mut reader: Box<dyn Read + 'a>,
) -> Result<(Box<dyn Read + 'a>, FileType, Option<FileType>), Error> {
    let (new_reader, file_type) = sniff_reader_filetype(reader);
    (new_reader, file_type, None)
}

/// Decompress a `Read` stream and returns the inferred file type
#[cfg(feature = "compression")]
pub fn decompress<'a>(
    reader: Box<dyn Read + 'a>,
) -> Result<(Box<dyn Read + 'a>, FileType, Option<FileType>), Error> {
    let (wrapped_reader, file_type) = sniff_reader_filetype(reader)?;
    Ok(match file_type {
        FileType::Gzip => {
            let gz_reader = MultiGzDecoder::new(wrapped_reader);
            let (new_reader, new_type) = sniff_reader_filetype(Box::new(gz_reader))?;
            (new_reader, new_type, Some(file_type))
        }
        FileType::Bzip => {
            let bz_reader = BzDecoder::new(wrapped_reader);
            let (new_reader, new_type) = sniff_reader_filetype(Box::new(bz_reader))?;
            (new_reader, new_type, Some(file_type))
        }
        FileType::Lzma => {
            let xz_reader = XzDecoder::new(wrapped_reader);
            let (new_reader, new_type) = sniff_reader_filetype(Box::new(xz_reader))?;
            (new_reader, new_type, Some(file_type))
        }
        FileType::Zstd => {
            let zstd_reader = ZstdDecoder::new(wrapped_reader)?;
            let (new_reader, new_type) = sniff_reader_filetype(Box::new(zstd_reader))?;
            (new_reader, new_type, Some(file_type))
        }
        x => (wrapped_reader, x, None),
    })
}
