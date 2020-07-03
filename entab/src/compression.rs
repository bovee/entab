use alloc::boxed::Box;
use std::io::Read;

#[cfg(feature = "compression")]
use bzip2::read::BzDecoder;
#[cfg(feature = "compression")]
use flate2::read::MultiGzDecoder;
#[cfg(feature = "lzma")]
use xz2::read::XzDecoder;
#[cfg(feature = "compression")]
use zstd::stream::read::Decoder as ZstdDecoder;

#[cfg(not(feature = "lzma"))]
pub use fake_compression::XzDecoder;
#[cfg(not(feature = "compression"))]
pub use fake_compression::{BzDecoder, MultiGzDecoder, ZstdDecoder};

use crate::filetype::{sniff_reader_filetype, FileType};
use crate::EtError;

/// Decompress a `Read` stream and returns the inferred file type
pub fn decompress<'a>(
    reader: Box<dyn Read + 'a>,
) -> Result<(Box<dyn Read + 'a>, FileType, Option<FileType>), EtError> {
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

#[cfg(all(test, feature = "compression", feature = "std"))]
mod tests {
    use super::*;
    use std::fs::File;

    #[test]
    fn test_read_gzip() -> Result<(), EtError> {
        let f = File::open("tests/data/test.bam")?;

        let (mut stream, _, compression) = decompress(Box::new(&f))?;
        assert_eq!(compression, Some(FileType::Gzip));
        let mut buf = Vec::new();
        assert_eq!(stream.read_to_end(&mut buf)?, 1392);
        Ok(())
    }
}

#[cfg(not(all(feature = "compression", feature = "lzma")))]
#[allow(dead_code)]
mod fake_compression {
    use std::io::Read;

    pub struct Fake;
    impl Fake {
        pub fn new<'r>(_: Box<dyn Read + 'r>) -> Self {
            Fake
        }
    }
    impl Read for Fake {
        fn read(&mut self, _: &mut [u8]) -> Result<usize, std::io::Error> {
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "entab was not compiled with support for compressed files",
            ))
        }
    }

    pub struct ZstdDecoder;
    impl ZstdDecoder {
        pub fn new<'r>(_: Box<dyn Read + 'r>) -> Result<Self, std::io::Error> {
            Ok(ZstdDecoder)
        }
    }
    impl Read for ZstdDecoder {
        fn read(&mut self, _: &mut [u8]) -> Result<usize, std::io::Error> {
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "entab was not compiled with support for compressed files",
            ))
        }
    }

    pub type BzDecoder = Fake;
    pub type MultiGzDecoder = Fake;
    pub type XzDecoder = Fake;
}
