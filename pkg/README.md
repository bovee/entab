# Entab

Parse record-based file formats into a stream of records.

## Usage

```javascript
import { Reader } from 'entab';

// now parse the file
const reader = new Reader(new Uint8Array(await file.arrayBuffer()));
// or a string
const reader = new Reader(new TextEncoder("utf-8").encode(">test\nacgt"));
for (const record of reader) {
  ...
}
```

Note that this will require paging the entire file into memory so files that
take >10 Mb may be slow and files >100 Mb may not work at all.

## Development

Build with `wasm-pack build`.

Test with `wasm-pack test`.

