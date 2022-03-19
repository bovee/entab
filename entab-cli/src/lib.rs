mod tsv_params;

use std::ffi::OsString;
use std::fs::File;
use std::io;
use std::str;

use clap::{crate_authors, crate_version, Arg, Command};
use clap::error::ErrorKind;
#[cfg(feature = "mmap")]
use memmap2::Mmap;

use entab::compression::decompress;
use entab::filetype::FileType;
use entab::readers::get_reader;
use entab::EtError;

use crate::tsv_params::TsvParams;

pub fn run<I, T, R, W>(args: I, stdin: R, stdout: W) -> Result<(), EtError>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
    R: io::Read,
    W: io::Write,
{
    let clap_result = Command::new("entab")
        .about("Turn anything into a TSV")
        .author(crate_authors!())
        .version(crate_version!())
        .arg(
            Arg::new("input")
                .short('i')
                .help("Path to read; if not provided stdin will be used")
                .takes_value(true),
        )
        .arg(
            Arg::new("output")
                .short('o')
                .help("Path to write to; if not provided stdout will be used")
                .takes_value(true),
        )
        .arg(
            Arg::new("parser")
                .short('p')
                .help("Parser to use [if not specified, file type will be auto-detected]")
                .takes_value(true),
        )
        .arg(
            Arg::new("metadata")
                .short('m')
                .long("metadata")
                .help("Reports metadata about the file instead of the data itself"),
        )
        .try_get_matches_from(args);

    let matches = match clap_result {
        Ok(d) => d,
        Err(e) => {
            if e.kind() == ErrorKind::DisplayHelp || e.kind() == ErrorKind::DisplayVersion {
                e.print()?;
                return Ok(());
            } else {
                return Err(e.to_string().into());
            }
        },
    };

    // TODO: map/reduce/filter options?
    // every column should either have a reduction set or it'll be dropped from
    // the result? reductions can be e.g. sum,average,count or group or column
    // (where column is the same as a pivot); this might be more useful as
    // another tool?

    #[cfg(feature = "mmap")]
    let mmap: Mmap;

    let (mut rb, _) = if let Some(i) = matches.value_of("input") {
        let file = File::open(i)?;
        #[cfg(feature = "mmap")]
        {
            mmap = unsafe { Mmap::map(&file)? };
            decompress(mmap.as_ref())?
        }
        #[cfg(not(feature = "mmap"))]
        decompress(file)?
    } else {
        let buffer: Box<dyn io::Read> = Box::new(stdin);
        decompress(buffer)?
    };
    let filetype = rb.sniff_filetype()?;
    let parser = matches
        .value_of("parser")
        .map_or_else(|| filetype, FileType::from_parser_name);
    let mut rec_reader = get_reader(parser, rb)?;
    // TODO: allow user to set these
    let params = TsvParams::default();

    let mut writer: Box<dyn io::Write> = if let Some(i) = matches.value_of("output") {
        Box::new(File::open(i)?)
    } else {
        Box::new(stdout)
    };

    if matches.is_present("metadata") {
        writer.write_all(b"key")?;
        writer.write_all(&[params.main_delimiter])?;
        writer.write_all(b"value")?;
        writer.write_all(&params.line_delimiter)?;
        for (key, value) in rec_reader.metadata() {
            params.write_str(key.as_bytes(), &mut writer)?;
            writer.write_all(&[params.main_delimiter])?;
            params.write_value(&value, &mut writer)?;
            writer.write_all(&params.line_delimiter)?;
        }
        return Ok(());
    }
    writer.write_all(
        rec_reader
            .headers()
            .join(str::from_utf8(&[params.main_delimiter])?)
            .as_bytes(),
    )?;
    writer.write_all(&params.line_delimiter)?;

    while let Some(fields) = rec_reader.next_record()? {
        params.write_value(&fields[0], &mut writer)?;
        for field in fields.iter().skip(1) {
            writer.write_all(&[params.main_delimiter])?;
            params.write_value(field, &mut writer)?;
        }
        writer.write_all(&params.line_delimiter)?;
    }
    writer.flush()?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() -> Result<(), EtError> {
        let mut out = Vec::new();
        assert!(run(["entab", "--version"], &b""[..], io::Cursor::new(&mut out)).is_ok());
        assert_eq!(&out[..], b"");
        Ok(())
    }

    #[test]
    fn test_output() -> Result<(), EtError> {
        let mut out = Vec::new();
        assert!(run(["entab"], &b">test\nACGT"[..], io::Cursor::new(&mut out)).is_ok());
        assert_eq!(&out[..], b"id\tsequence\ntest\tACGT\n");
        Ok(())
    }

    #[test]
    fn test_metadata() -> Result<(), EtError> {
        let mut out = Vec::new();
        assert!(run(["entab", "--metadata"], &b">test\nACGT"[..], io::Cursor::new(&mut out)).is_ok());
        assert_eq!(&out[..], b"key\tvalue\n");
        Ok(())
    }
}
