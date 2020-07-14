#[cfg(feature = "std")]
use alloc::boxed::Box;
#[cfg(feature = "std")]
use alloc::vec;
#[cfg(feature = "std")]
use std::io::{Cursor, Read};

#[cfg(feature = "std")]
use crate::buffer::BUFFER_SIZE;
#[cfg(feature = "std")]
use crate::EtError;

#[cfg(feature = "std")]
pub fn sniff_reader_filetype<'a>(
    mut reader: Box<dyn Read + 'a>,
) -> Result<(Box<dyn Read + 'a>, FileType), EtError> {
    let mut first = vec![0; BUFFER_SIZE];
    let amt_read = reader.read(&mut first)?;
    unsafe {
        first.set_len(amt_read);
    }

    let file_type = FileType::from_magic(&first);
    Ok((Box::new(Cursor::new(first).chain(reader)), file_type))
}

/// A file format.
#[derive(Debug, PartialEq)]
pub enum FileType {
    // compression
    Gzip,
    Bzip,
    Lzma,
    Zstd,
    // bioinformatics
    Bam,
    Fasta,
    Fastq,
    Facs,
    Sam,
    // chemoinformatics
    AgilentMs,       // ms   0x01, 0x32
    AgilentMsMsScan, // bin   0x01, 0x01
    AgilentChemstation,
    AgilentCsDad, // uv   0x02, 0x33
    AgilentDad,   // sd
    AgilentFid,
    AgilentMwd,  // ch   0x02, 0x33
    AgilentMwd2, // ch   0x03, 0x31
    BrukerBaf,
    BrukerMsms,
    InficonHapsite,
    ThermoCf,
    ThermoDxf,
    ThermoRaw,
    WatersAutospec,
    NetCdf,
    MzXml,
    // geology
    Las,
    // catch all
    Hdf5,
    Tsv,
    Unknown,
}

impl FileType {
    /// Given a slice from the beginning of the file, try to guess which file
    /// format that file is in.
    pub fn from_magic(magic: &[u8]) -> FileType {
        if magic.len() > 8 {
            match &magic[..8] {
                b"~VERSION" => return FileType::Las,
                b"~Version" => return FileType::Las,
                b"\x89HDF\r\n\x1A\n" => return FileType::Hdf5,
                _ => {}
            }
        }
        if magic.len() > 4 {
            match &magic[..4] {
                b"BAM\x01" => return FileType::Bam,
                b"FCS3" => return FileType::Facs,
                b"@HD\t" => return FileType::Sam,
                b"@SQ\t" => return FileType::Sam,
                [0xFD, 0x2F, 0xB5, 0x28] => return FileType::Zstd,
                [0xFF, 0xFF, 0x60, 0x00] => return FileType::ThermoDxf,
                _ => {}
            }
        }
        if magic.len() < 2 {
            return FileType::Unknown;
        }
        match &magic[..2] {
            [0x1F, 0x8B] => return FileType::Gzip,
            [0x0F, 0x8B] => return FileType::Gzip,
            [0x42, 0x5A] => return FileType::Bzip,
            [0xFD, 0x37] => return FileType::Lzma,
            [0x01, 0x32] => return FileType::AgilentChemstation,
            [0x02, 0x38] => return FileType::AgilentFid,
            [0x24, 0x00] => return FileType::BrukerBaf,
            // TODO: better logic to handle these kinds of different types/same magic cases
            // (this is the same 2 byte start as ThermoDxf)
            [0xFF, 0xFF] => return FileType::ThermoCf,
            [0x01, 0xA1] => return FileType::ThermoRaw,
            [0x04, 0x03] => return FileType::InficonHapsite,
            [0x43, 0x44] => return FileType::NetCdf,
            _ => {}
        }
        match &magic[..1] {
            b">" => FileType::Fasta,
            b"@" => FileType::Fastq,
            _ => FileType::Unknown,
        }
    }

    /// Return the list of possible file extensions a given file format
    /// could have.
    pub fn extensions(&self) -> &[&str] {
        match self {
            FileType::Gzip => &["gz", "gzip"],
            FileType::Bzip => &["bz", "bz2", "bzip"],
            FileType::Lzma => &["xz"],
            FileType::Zstd => &["zstd"],
            FileType::AgilentChemstation => &["ms"],
            FileType::AgilentFid => &["ch"],
            FileType::Bam => &["bam"],
            FileType::BrukerBaf => &["baf"],
            FileType::BrukerMsms => &["ami"],
            FileType::Facs => &["fcs", "lmd"],
            FileType::Fasta => &["fa", "fasta", "fna", "faa"],
            FileType::Fastq => &["fq", "fastq"],
            FileType::Hdf5 => &["hdf"],
            FileType::MzXml => &["mzxml"],
            FileType::NetCdf => &["cdf"],
            FileType::InficonHapsite => &["hps"],
            FileType::Sam => &["sam"],
            FileType::ThermoCf => &["cf"],
            FileType::ThermoDxf => &["dxf"],
            FileType::ThermoRaw => &["raw"],
            FileType::WatersAutospec => &["idx"],
            _ => &[""],
        }
    }

    /// Returns the "best" parser for a given file
    pub fn to_parser_name(&self) -> &str {
        match self {
            FileType::AgilentChemstation => "chemstation",
            FileType::Bam => "bam",
            FileType::Fasta => "fasta",
            FileType::Fastq => "fastq",
            FileType::Sam => "sam",
            FileType::ThermoDxf => "dxf",
            FileType::Tsv => "tsv",
            _ => "",
        }
    }
}
