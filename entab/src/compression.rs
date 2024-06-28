#[cfg(feature = "std")]
use alloc::boxed::Box;
use core::convert::TryInto;

#[cfg(all(feature = "compression", feature = "std"))]
use bzip2::read::BzDecoder;
#[cfg(feature = "std")]
use flate2::read::MultiGzDecoder;
#[cfg(all(feature = "compression", feature = "std"))]
use xz2::read::XzDecoder;
#[cfg(all(feature = "compression", feature = "std"))]
use zstd::stream::read::Decoder as ZstdDecoder;

use crate::buffer::ReadBuffer;
use crate::filetype::FileType;
use crate::EtError;

/// Decompress the contents of a `ReadBuffer` into a new `ReadBuffer` and return the type of compression.
///
/// # Errors
/// If reading fails or if the stream can't be decompressed, return `EtError`.
#[cfg(all(feature = "compression", feature = "std"))]
pub fn decompress<'r, B>(data: B) -> Result<(ReadBuffer<'r>, Option<FileType>), EtError>
where
    B: TryInto<ReadBuffer<'r>>,
    EtError: From<<B as TryInto<ReadBuffer<'r>>>::Error>,
{
    let mut reader = data.try_into()?;
    let file_type = reader.sniff_filetype()?;
    Ok(match file_type {
        FileType::Gzip => {
            let gz_reader = MultiGzDecoder::new(reader.into_box_read());
            (
                ReadBuffer::from_reader(Box::new(gz_reader), None)?,
                Some(file_type),
            )
        }
        FileType::Bzip => {
            let bz_reader = BzDecoder::new(reader.into_box_read());
            (
                ReadBuffer::from_reader(Box::new(bz_reader), None)?,
                Some(file_type),
            )
        }
        FileType::Lzma => {
            let xz_reader = XzDecoder::new(reader.into_box_read());
            (
                ReadBuffer::from_reader(Box::new(xz_reader), None)?,
                Some(file_type),
            )
        }
        FileType::Zstd => {
            let zstd_reader = ZstdDecoder::new(reader.into_box_read())?;
            (
                ReadBuffer::from_reader(Box::new(zstd_reader), None)?,
                Some(file_type),
            )
        }
        _ => (reader, None),
    })
}

/// Decompress a `Read` stream and returns the inferred file type.
///
/// # Errors
/// If reading fails or if the stream can't be decompressed, return `EtError`.
#[cfg(all(not(feature = "compression"), feature = "std"))]
pub fn decompress<'r, B>(data: B) -> Result<(ReadBuffer<'r>, Option<FileType>), EtError>
where
    B: TryInto<ReadBuffer<'r>>,
    EtError: From<<B as TryInto<ReadBuffer<'r>>>::Error>,
{
    let mut reader = data.try_into()?;
    let file_type = reader.sniff_filetype()?;
    Ok(match file_type {
        FileType::Gzip => {
            let gz_reader = MultiGzDecoder::new(reader.into_box_read());
            (
                ReadBuffer::from_reader(Box::new(gz_reader), None)?,
                Some(file_type),
            )
        }
        FileType::Bzip | FileType::Lzma | FileType::Zstd => {
            return Err("entab was not compiled with support for compressed files".into());
        }
        _ => (reader, None),
    })
}

/// Decompress a `Read` stream and returns the inferred file type.
///
/// # Errors
/// If reading fails or if the stream can't be decompressed, return `EtError`.
#[cfg(not(feature = "std"))]
pub fn decompress<'r, B>(data: B) -> Result<(ReadBuffer<'r>, Option<FileType>), EtError>
where
    B: TryInto<ReadBuffer<'r>>,
    EtError: From<<B as TryInto<ReadBuffer<'r>>>::Error>,
{
    let mut reader = data.try_into()?;
    let file_type = reader.sniff_filetype()?;
    Ok(match file_type {
        FileType::Gzip | FileType::Bzip | FileType::Lzma | FileType::Zstd => {
            return Err("entab was not compiled with support for any compressed files".into());
        }
        _ => (reader, None),
    })
}

#[cfg(all(test, feature = "compression", feature = "std"))]
mod tests {
    use super::*;
    use std::fs::File;

    #[test]
    fn test_read_gzip() -> Result<(), EtError> {
        let f = File::open("tests/data/test.bam")?;

        let (mut rb, compression) = decompress(f)?;
        assert_eq!(compression, Some(FileType::Gzip));
        let x: &[u8] = rb.next(&mut 1392)?.unwrap();
        assert_eq!(x.len(), 1392);
        assert!(rb.next::<&[u8]>(&mut 1).is_err());
        Ok(())
    }

    #[test]
    fn test_read_bzip2() -> Result<(), EtError> {
        let f = File::open("tests/data/test.csv.bz2")?;

        let (rb, compression) = decompress(f)?;
        assert_eq!(compression, Some(FileType::Bzip));
        assert_eq!(rb.as_ref().len(), 48);
        Ok(())
    }

    #[test]
    fn test_read_xz() -> Result<(), EtError> {
        let f = File::open("tests/data/test.csv.xz")?;

        let (rb, compression) = decompress(f)?;
        assert_eq!(compression, Some(FileType::Lzma));
        assert_eq!(rb.as_ref().len(), 48);
        Ok(())
    }

    #[test]
    fn test_read_zstd() -> Result<(), EtError> {
        let f = File::open("tests/data/test.csv.zst")?;

        let (rb, compression) = decompress(f)?;
        assert_eq!(compression, Some(FileType::Zstd));
        assert_eq!(rb.as_ref().len(), 48);
        Ok(())
    }
}
