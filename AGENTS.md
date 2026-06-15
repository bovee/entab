# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

### Build
```sh
cargo build                    # build the whole workspace
cargo build -p entab           # build only the core library
cargo build -p entab-cli       # build the CLI
```

### Test
```sh
cargo test                     # run all workspace tests
cargo test -p entab            # run tests for core library only
cargo test --no-default-features  # run no_std tests (important for verifying no_std compat)
cargo test <test_name>         # run a specific test by name
```

### JS bindings
```sh
wasm-pack build                # build the WASM package (in entab-js/)
wasm-pack test                 # test the WASM bindings
```

### Python bindings
```sh
maturin build                  # build the Python wheel
maturin develop                # install a working dev copy
```

## Architecture

The workspace is organized as:
- **`entab/`** — core library (no external service dependencies, supports `no_std`)
- **`entab-cli/`** — CLI binary using the core library, outputs TSV
- **`entab-js/`** — WebAssembly bindings via `wasm-bindgen`, compiled with `wasm-pack`
- **`entab-py/`** — Python bindings via `maturin`/PyO3
- **`entab-r/`** — R bindings (excluded from workspace; build separately)
- **`entab-benchmarks/`** — benchmarks (excluded from workspace)

### Core library (`entab/src/`)

The core parses binary/text file formats into a stream of typed records. Two usage modes:

1. **Typed (fast):** Use a format-specific `*Reader` struct directly (e.g., `FastaReader`) which returns strongly-typed records (e.g., `FastaRecord`).
2. **Dynamic (generic):** Use `get_reader()` from `readers.rs`, which returns a `Box<dyn RecordReader>` yielding `Vec<Value>` rows. This is what language bindings use.

Key modules:
- **`buffer.rs`** — `ReadBuffer<'r>`: the streaming buffer abstraction. Wraps `Box<dyn Read>` with a refillable internal buffer. The `next<T>()` method drives parsing by calling `T::parse()` then `T::get()`.
- **`parsers/mod.rs`** — `FromSlice` trait: the core parsing trait. Each parser implements `parse()` (checks if enough data is present, advances `consumed`) and `get()` (populates the struct from the slice). The separation allows zero-copy parsing.
- **`readers.rs`** — `RecordReader` trait and `get_reader()`. The `impl_reader!` macro generates `*Reader<'r>` structs automatically. `impl_record!` macro generates `Vec<Value>` conversions.
- **`record.rs`** — `Value<'a>` enum (Null, Boolean, Datetime, Float, Integer, String, List, Record) used as the generic row type.
- **`filetype.rs`** — `FileType` enum and magic-byte detection via `sniff_filetype()`.
- **`compression.rs`** — transparent decompression (gzip, bzip2, xz, zstd).

### Parser pattern

Each parser (e.g., `fasta.rs`) follows this pattern:
1. A `*State` struct (implements `FromSlice` with `State = ()`) reads the file header/initialization data.
2. A `*Record<'r>` struct (implements `FromSlice` with `State = *State`) reads individual records.
3. `impl_reader!(FastaReader, FastaRecord<'r>, FastaRecord<'r>, FastaState, ())` generates the public reader.
4. `impl_record!(FastaRecord<'r>: id, sequence)` generates the `Vec<Value>` conversion.

The `'r` lifetime is borrowed from the `ReadBuffer`, enabling zero-copy access to buffer contents.

### `no_std` support

The core `entab` crate is `no_std` compatible (uses `extern crate alloc`). The `std` feature enables `Box<dyn Read>` streaming; without it, parsing works only on `&[u8]` slices. The `png` parser is `#[cfg(feature = "std")]` only. Test `no_std` compat with `cargo test --no-default-features`.
