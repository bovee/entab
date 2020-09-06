# entab

Parse record-based file formats into a stream of records.

## Usage

```python
from entab import Reader
reader = Reader(filename='test.fa')
for record in reader:
    ...
```

## Development

Build with `maturin build --cargo-extra-args=--features=maturin` or build
a working copy with `maturin develop --cargo-extra-args=--features=maturin`.

Test with `cargo test`.
