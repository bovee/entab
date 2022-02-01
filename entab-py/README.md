# Entab

Parse record-based file formats into a stream of records.

## Usage

```python
from entab import Reader
reader = Reader(filename='test.fa')
for record in reader:
    print(record.id)
```

## Development

Build with `maturin build --cargo-extra-args=--features=maturin` or build
a working copy with `maturin develop --cargo-extra-args=--features=maturin`.

Test with `cargo test`.

# Releases

Binary wheels can be built by running the following from the workspace root directory (one up):
`docker run --rm -v $(pwd):/io konstin2/maturin:v0.12.6 build -m entab-py/Cargo.toml --cargo-extra-args=--features=maturin --no-sdist`
