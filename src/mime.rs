
pub enum FileType {
    // compression
    Gzip,
    Bzip,
    Lzma,
    // bioinformatics
    Fasta,
    Fastq,
    Facs,
    // chemoinformatics
    AgilentMs,    // ms   0x01, 0x32
    AgilentMsMsScan,    // bin   0x01, 0x01
    AgilentChemstation,
    AgilentCsDad, // uv   0x02, 0x33
    AgilentDad,   // sd
    AgilentFid,
    AgilentMwd,   // ch   0x02, 0x33
    AgilentMwd2,  // ch   0x03, 0x31
    BrukerBaf,
    BrukerMsms,
    InficonHapsite,
    ThermoCf,
    ThermoDxf,
    ThermoRaw,
    WatersAutospec,
    NetCdf,
    MzXml,
    // catch all
    Unknown,
}


impl FileType {
    pub fn from_magic(magic: &[u8]) -> FileType {
        match &magic[0..4] {
            &[b'F', b'C', b'S', b'3'] => return FileType::Facs,
            _ => {},
        }
        if magic.len() < 2 {
            return FileType::Unknown;
        }
        match &magic[0..2] {
            &[0x0F, 0x8B] => return FileType::Gzip,
            &[0x42, 0x5A] => return FileType::Bzip,
            &[0xFD, 0x37] => return FileType::Lzma,
            &[0x01, 0x32] => return FileType::AgilentChemstation,
            &[0x02, 0x38] => return FileType::AgilentFid,
            &[0x24, 0x00] => return FileType::BrukerBaf,
            &[0xFF, 0xFF] => return FileType::ThermoCf,
            &[0xFF, 0xFF] => return FileType::ThermoDxf,
            &[0x01, 0xA1] => return FileType::ThermoRaw,
            &[0x04, 0x03] => return FileType::InficonHapsite,
            &[0x43, 0x44] => return FileType::NetCdf,
            _ => {},
        }
        match &magic[0..1] {
            &[b'>'] => FileType::Fasta,
            &[b'@'] => FileType::Fastq,
            _ => FileType::Unknown,
        }
    }

    pub fn extensions(&self) -> &[&str] {
        match self {
            FileType::Gzip => &["gz", "gzip"],
            FileType::Bzip => &["bz", "bz2", "bzip"],
            FileType::Lzma => &["xz"],
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
}
