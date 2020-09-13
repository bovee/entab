# entab
What is everything were a/could be turned into a table?

entab is a parsing framework to turn a variety of record-based scientific file
formats into usable tabular data across a variety of programming languages.

![Test status](https://github.com/bovee/entab/workflows/Tests/badge.svg)
[![Package on Crates.io](https://img.shields.io/crates/v/entab.svg)](https://crates.io/crates/entab)
[![Package on NPM](https://img.shields.io/npm/v/entab.svg)](https://www.npmjs.com/package/entab)
[![Package on PyPI](https://img.shields.io/pypi/v/entab.svg)](https://pypi.org/project/entab/)

## Formats

entab supports reading a variety of bioinformatics, chemoinformatics, and
other formats.

 - Agilent Chemstation CH, FID, MS, MWD, and UV formats
 - FASTA and FASTQ sequence formats
 - FCS flow cytometry format
 - Inficon Hapsite mass specotrometry format
 - SAM and BAM alignment formats
 - Thermo continuous flow isotope mass spectrometry formats
 - TSV

## CLI

entab has a CLI that allows piping in arbitrary files and outputs TSVs.
Install with:
```bash
cargo install entab-cli
```

Example usage to see how many records are in a file:
```bash
cat test.fa | entab | sed '1d' | wc -l
```

## Bindings

There are bindings for two languages, Python and JavaScript, that support
reading data streams and converting them into a series of records.

The Javascript library can be installed with:
```
npm install entab
```
The Python library can be installed with:
```
pip install entab
```

## Priorities

1. *Handling many formats:*
    Support as many record-based, streamable scientific formats as possible.
    Formats like HDF5 with complex headers and already existing, well-supported
    parsers are not considered a priority though.

2. *Correctness:*
     Formats should be parsed with good error messages, consistant failure
     states, and well-tested code.

3. *Language bindings:*
     Support using entab from a decent selection of the programming languages
     currently used for science, data science, and related fields. Currently
     supporting Python and Javascript with possible support for Julia and R
     in the future.

5. *Speed:*
     entab should be as fast as possible while still prioritizing the above
     issues. Parsers are split into two forms: a fast one that produces a
     specialized struct and a slow one that produces a generic record and is
     capable of being switched to at run time.
