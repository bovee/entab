# entab
What is everything were a/could be turned into a tsv?

## Formats

entab supports reading a variety of bioinformatics, chemoinformatics, and
other formats.

## CLI

entab has a CLI that allows piping in arbitrary files and outputs TSVs.
Install with:
```bash
cargo install entab-cli
```

Use like:
```bash
cat test.fa | entab | sed '1d' | wc -l
```

## Bindings

There are bindings for two languages, Python and JavaScript, that support
reading data streams and converting them into a series of records.

The Javascript library can be installed with `npm install entab` and the
Python library can be installed with `pip install entab`.

## Priorities

1. Covering many formats
     Support as many record-based, streamable formats as possible. Formats
     like HDF5 with complex headers and already existing, well-supported
     parsers are not considered a priority though.

2. Correctness
     Formats should be parsed with good error messages, consistant failure
     states, and well-tested code.

3. Language bindings
     Support using entab from a decent selection of the programming languages
     currently used for science, data science, and related fields. Currently
     supporting Python and Javascript with possible support for Julia and R
     in the future.

5. Speed
     entab should be as fast as possible while still prioritizing the above
     issues. Parsers are split into two forms: a fast one that produces a
     specialized struct and a slow one that produces a generic record and is
     capable of being switched to at run time.
