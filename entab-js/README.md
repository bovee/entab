# Entab

Parse record-based file formats into a stream of records.

## Usage

```javascript
import { Reader } from 'entab';
// patch Reader to be an iterator too
Reader.prototype[Symbol.iterator] = function() { return this; };

// now parse the file
const reader = new Reader(new Uint8Array(await file.arrayBuffer()));
for (const record of reader) {
  ...
}
```

## Development

Build with `wasm-pack build`.

Test with `wasm-pack test`.

