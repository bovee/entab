use std::fs::File;
use std::io;
use std::io::Write;

use clap::{crate_authors, crate_version, App, Arg};

use entab::buffer::ReadBuffer;
use entab::compression::decompress;
use entab::filetype::FileType;
use entab::readers;
use entab::record::{BindT, ReaderBuilder, Record};
use entab::EtError;

macro_rules! all_types {
    (match $m:expr => $f:ident::<$($t:ty)*>($($arg:expr),*)) => {
        match $m {
            FileType::Fasta => $f::<readers::fasta::FastaReaderBuilder,$($t),*>($($arg),*),
            _ => $f::<readers::tsv::TsvReaderBuilder,$($t),*>($($arg),*),
        }
    };
}

pub fn write_reader_to_tsv<R, W>(buffer: ReadBuffer, writer: &mut W) -> Result<(), EtError>
where
    R: ReaderBuilder,
    R::Item: for<'a> BindT<'a>,
    W: Write,
{
    let mut rec_reader = R::default().to_reader(buffer)?;
    writer.write_all(&rec_reader.headers().join("\t").as_bytes())?;
    while let Some(n) = rec_reader.next()? {
        writer.write_all(b"\n")?;
        n.write_field(0, writer)?;
        for i in 1..n.size() {
            writer.write_all(b"\t")?;
            n.write_field(i, writer)?;
        }
    }
    Ok(())
}

pub fn main() -> Result<(), EtError> {
    let matches = App::new("entab")
        .about("Turn anything into a TSV")
        .author(crate_authors!())
        .version(crate_version!())
        .arg(
            Arg::with_name("input")
                .short('i')
                .about("Path to read; if not provided stdin will be used")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("output")
                .short('o')
                .about("Path to write to; if not provided stdout will be used")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("parser")
                .short('p')
                .about("Parser to use [if not specified, file type will be auto-detected]")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("metadata")
                .short('m')
                .about("Reports metadata about the file"),
        )
        .get_matches();

    // TODO: map/reduce/filter options?
    // every column should either have a reduction set or it'll be dropped from
    // the result? reductions can be e.g. sum,average,count or group or column
    // (where column is the same as a pivot)

    // stdin needs to be out here for lifetime purposes
    let stdin = io::stdin();
    let stdout = io::stdout();

    let (rb, filetype) = if let Some(i) = matches.value_of("input") {
        let file = File::open(i)?;
        let (reader, filetype, _) = decompress(Box::new(file))?;
        (ReadBuffer::new(reader)?, filetype)
    } else {
        let locked_stdin = stdin.lock();
        let (reader, filetype, _) = decompress(Box::new(locked_stdin))?;
        (ReadBuffer::new(reader)?, filetype)
    };

    let mut writer: Box<dyn Write> = if let Some(i) = matches.value_of("output") {
        Box::new(File::open(i)?)
    } else {
        Box::new(stdout.lock())
    };

    if matches.is_present("metadata") {
        // TODO: get the compression from above too
        // TODO: print metadata
        return Ok(());
    } else {
        all_types!(match filetype => write_reader_to_tsv::<_>(rb, &mut writer))?;
    }

    writer.flush()?;

    Ok(())
}
