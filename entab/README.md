# entab
This is the main file parsing library and includes support for compression/
decompression, file type inference, and parsers for different file types.

## Usage

```rust
use std::fs::File;
use entab::{ReadBuffer, Record};
use entab::readers::fasta::FastaReaderBuilder;

let buffer = ReadBuffer::new(File::open("./tests/data/test.fasta"))?;
let reader = FastaReaderBuilder::to_reader(buffer)?;
while let Some(Record::Fasta { id, .. }) = reader.next()? {
    println!("{}", id);
}
```

## Other Parsers
[Aston](https://github.com/bovee/aston) - Python - Agilent Chemstation & Masshunter/Thermo DXF/Inficon/etc
[Chromatography Toolbox](https://github.com/chemplexity/chromatography) - Matlab - Agilent/Thermo/NetCDF/mzXML
[Isoreader](https://github.com/isoverse/isoreader) - R - Isodat
[Unfinnigan](https://github.com/prvst/unfinnigan) - Perl/Python - Thermo RAW
