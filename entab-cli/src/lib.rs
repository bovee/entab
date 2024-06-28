mod tsv_params;

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs::File;
use std::io;
use std::str;

use clap::error::ErrorKind;
use clap::{crate_authors, crate_version, Arg, Command};
#[cfg(feature = "mmap")]
use memmap2::Mmap;

use entab::readers::get_reader;
use entab::record::Value;
use entab::EtError;

use crate::tsv_params::TsvParams;

/// Parse the provided `stdin` using `args` and write results to `stdout`.
///
/// # Errors
/// If there are any issues, an `EtError` will be returned.
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
                .num_args(1),
        )
        .arg(
            Arg::new("output")
                .short('o')
                .help("Path to write to; if not provided stdout will be used")
                .num_args(1),
        )
        .arg(
            Arg::new("parser")
                .short('p')
                .help("Parser to use [if not specified, it will be auto-detected]")
                .num_args(1),
        )
        .arg(
            Arg::new("metadata")
                .short('m')
                .long("metadata")
                .help("Reports metadata about the file instead of the data itself")
                .action(clap::ArgAction::SetTrue),
        )
        .try_get_matches_from(args);

    let matches = match clap_result {
        Ok(d) => d,
        Err(e) => {
            if e.kind() == ErrorKind::DisplayHelp || e.kind() == ErrorKind::DisplayVersion {
                e.print()?;
                return Ok(());
            }
            return Err(e.to_string().into());
        }
    };

    // TODO: map/reduce/filter options?
    // every column should either have a reduction set or it'll be dropped from
    // the result? reductions can be e.g. sum,average,count or group or column
    // (where column is the same as a pivot); this might be more useful as
    // another tool?

    #[cfg(feature = "mmap")]
    let mmap: Mmap;

    let mut parse_params = BTreeMap::new();
    let parser = matches.get_one::<&str>("parser").copied();
    let (mut rec_reader, _) = if let Some(&i) = matches.get_one::<&str>("input") {
        parse_params.insert("filename".to_string(), Value::String(i.into()));
        let file = File::open(i)?;
        #[cfg(feature = "mmap")]
        {
            mmap = unsafe { Mmap::map(&file)? };
            get_reader(mmap.as_ref(), parser, Some(parse_params))?
        }
        #[cfg(not(feature = "mmap"))]
        get_reader(file, parser, Some(parse_params))?
    } else {
        let buffer: Box<dyn io::Read> = Box::new(stdin);
        get_reader(buffer, parser, Some(parse_params))?
    };
    // TODO: allow user to set these
    let params = TsvParams::default();

    let mut writer: Box<dyn io::Write> = if let Some(&i) = matches.get_one::<&str>("output") {
        Box::new(File::create(i)?)
    } else {
        Box::new(stdout)
    };

    if matches.get_flag("metadata") {
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
        println!("{}", std::str::from_utf8(&out).unwrap());
        assert_eq!(&out[..], b"id\tsequence\ntest\tACGT\n");
        Ok(())
    }

    #[test]
    fn test_metadata() -> Result<(), EtError> {
        let mut out = Vec::new();
        run(
            ["entab", "--metadata"],
            &b">test\nACGT"[..],
            io::Cursor::new(&mut out)
        )?;
        assert_eq!(&out[..], b"key\tvalue\n");
        Ok(())
    }
}
