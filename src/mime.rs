use std::io::{Cursor, Error, Read};

use crate::buffer::BUFFER_SIZE;

pub fn sniff_reader_filetype<'a>(
    mut reader: Box<dyn Read + 'a>,
) -> Result<(Box<dyn Read + 'a>, FileType), Error> {
    let mut first = vec![0; BUFFER_SIZE];
    let amt_read = reader.read(&mut first)?;
    unsafe {
        first.set_len(amt_read);
    }

    let file_type = FileType::from_magic(&first);
    Ok((Box::new(Cursor::new(first).chain(reader)), file_type))
}

#[derive(Debug, PartialEq)]
pub enum FileType {
    // compression
    Gzip,
    Bzip,
    Lzma,
    Zstd,
    // bioinformatics
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
    Tsv,
    Unknown,
}

impl FileType {
    pub fn from_magic(magic: &[u8]) -> FileType {
        match &magic[0..8] {
            b"~VERSION" => return FileType::Las,
            b"~Version" => return FileType::Las,
            _ => {}
        }
        match &magic[0..4] {
            b"FCS3" => return FileType::Facs,
            b"@HD\t" => return FileType::Sam,
            [0xFD, 0x2F, 0xB5, 0x28] => return FileType::Zstd,
            _ => {}
        }
        if magic.len() < 2 {
            return FileType::Unknown;
        }
        match &magic[0..2] {
            [0x0F, 0x8B] => return FileType::Gzip,
            [0x42, 0x5A] => return FileType::Bzip,
            [0xFD, 0x37] => return FileType::Lzma,
            [0x01, 0x32] => return FileType::AgilentChemstation,
            [0x02, 0x38] => return FileType::AgilentFid,
            [0x24, 0x00] => return FileType::BrukerBaf,
            [0xFF, 0xFF] => return FileType::ThermoCf,
            // TODO: better logic to handle these kinds of different types/same magic cases
            // [0xFF, 0xFF] => return FileType::ThermoDxf,
            [0x01, 0xA1] => return FileType::ThermoRaw,
            [0x04, 0x03] => return FileType::InficonHapsite,
            [0x43, 0x44] => return FileType::NetCdf,
            _ => {}
        }
        match &magic[0..1] {
            b">" => FileType::Fasta,
            b"@" => FileType::Fastq,
            _ => FileType::Unknown,
        }
    }

    pub fn extensions(&self) -> &[&str] {
        match self {
            FileType::Gzip => &["gz", "gzip"],
            FileType::Bzip => &["bz", "bz2", "bzip"],
            FileType::Lzma => &["xz"],
            FileType::Zstd => &["zstd"],
            FileType::Fasta => &["fa", "fasta", "fna", "faa"],
            FileType::Fastq => &["fq", "fastq"],
            FileType::AgilentChemstation => &["ms"],
            FileType::AgilentFid => &["ch"],
            FileType::BrukerBaf => &["baf"],
            FileType::BrukerMsms => &["ami"],
            FileType::InficonHapsite => &["hps"],
            FileType::ThermoCf => &["cf"],
            FileType::ThermoDxf => &["dxf"],
            FileType::ThermoRaw => &["raw"],
            FileType::WatersAutospec => &["idx"],
            FileType::NetCdf => &["cdf"],
            FileType::MzXml => &["mzxml"],
            FileType::Facs => &["fcs", "lmd"],
            _ => &[""],
        }
    }

    pub fn to_parser_name(&self) -> &str {
        match self {
            FileType::Fasta => "fasta",
            FileType::Fastq => "fastq",
            FileType::Sam => "sam",
            FileType::Tsv => "tsv",
            _ => "",
        }
    }
}
