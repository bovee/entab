# Benchmarks

These are benchmarks of entab against other Rust parsers available on crates.io.
Note that the test files used as _not_ representative of real life data, e.g. needletail appears very slow parsing FASTA files here because of set-up overhead, but with more representative, real-life data it's much, much faster.
To test on more accurate scenarios, update the filenames in benchmarks.rs.

Run with `cargo criterion`.

