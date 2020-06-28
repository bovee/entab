use std::fs::File;
use std::io;

use clap::{crate_authors, crate_version, App, Arg};
#[cfg(feature = "mmap")]
use memmap::Mmap;

use entab::buffer::ReadBuffer;
use entab::compression::decompress;
use entab::filetype::FileType;
use entab::readers::get_builder;
use entab::EtError;

pub fn write_reader_to_tsv<W>(
    buffer: ReadBuffer,
    filetype: FileType,
    mut write: W,
) -> Result<(), EtError>
where
    W: FnMut(&[u8]) -> Result<(), EtError>,
{
    let mut rec_reader = if let Some(builder) = get_builder(filetype.to_parser_name()) {
        builder.to_reader(buffer)?
    } else {
        return Err("No parser could not be found for the data provided".into());
    };

    write(&rec_reader.headers().join("\t").as_bytes())?;
    write(b"\n")?;
    while let Some(n) = rec_reader.next()? {
        n.write_field(0, &mut write)?;
        for i in 1..n.size() {
            write(b"\t")?;
            n.write_field(i, &mut write)?;
        }
        write(b"\n")?;
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
    #[cfg(feature = "mmap")]
    let mmap: Mmap;

    let (rb, filetype, _) = if let Some(i) = matches.value_of("input") {
        let file = File::open(i)?;
        let (reader, filetype, compression) = decompress(Box::new(file))?;
        if compression == None {
            // if the file's decompressed already, re-open it as a mmap
            #[cfg(feature = "mmap")]
            {
                let file = File::open(i)?;
                mmap = unsafe { Mmap::map(&file)? };
                (ReadBuffer::from_slice(&mmap), filetype, compression)
            }
            #[cfg(not(feature = "mmap"))]
            (ReadBuffer::new(reader)?, filetype, compression)
        } else {
            (ReadBuffer::new(reader)?, filetype, compression)
        }
    } else {
        let locked_stdin = stdin.lock();
        let (reader, filetype, compression) = decompress(Box::new(locked_stdin))?;
        (ReadBuffer::new(reader)?, filetype, compression)
    };

    let mut writer: Box<dyn io::Write> = if let Some(i) = matches.value_of("output") {
        Box::new(File::open(i)?)
    } else {
        Box::new(stdout.lock())
    };
    let write = |buf: &[u8]| -> Result<(), EtError> { Ok(writer.write_all(buf)?) };

    if matches.is_present("metadata") {
        // TODO: get the compression from above too
        // TODO: print metadata
        return Ok(());
    } else {
        write_reader_to_tsv(rb, filetype, write)?;
    }

    writer.flush()?;

    Ok(())
}
