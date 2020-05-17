use std::io;
use std::io::Write;

use entab::compression::decompress;

pub fn main() -> Result<(), io::Error> {
    let stdin = io::stdin();
    let locked_stdin = stdin.lock();
    let (_, _, _) = decompress(Box::new(locked_stdin))?;

    let stdout = io::stdout();
    let mut locked_stdout = stdout.lock();

    // record_reader.write_tsv(&mut locked_stdout)?;
    locked_stdout.flush()?;

    Ok(())
}
