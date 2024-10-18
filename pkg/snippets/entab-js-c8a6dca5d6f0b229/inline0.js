
  export function make_reader_iter(proto) { proto[Symbol.iterator] = function () { return this; }; }
