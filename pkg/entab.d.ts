/* tslint:disable */
/* eslint-disable */
/**
*/
export function start(): void;
/**
*/
export class Reader {
  free(): void;
/**
* @param {Uint8Array} data
* @param {string | undefined} [parser]
*/
  constructor(data: Uint8Array, parser?: string);
/**
* @returns {any}
*/
  next(): any;
/**
*/
  readonly headers: any;
/**
*/
  readonly metadata: any;
/**
*/
  readonly parser: string;
}

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
  readonly memory: WebAssembly.Memory;
  readonly __wbg_reader_free: (a: number) => void;
  readonly reader_new: (a: number, b: number, c: number, d: number, e: number) => void;
  readonly reader_parser: (a: number, b: number) => void;
  readonly reader_headers: (a: number) => number;
  readonly reader_metadata: (a: number, b: number) => void;
  readonly reader_next: (a: number, b: number) => void;
  readonly start: () => void;
  readonly __wbindgen_add_to_stack_pointer: (a: number) => number;
  readonly __wbindgen_malloc: (a: number, b: number) => number;
  readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
  readonly __wbindgen_free: (a: number, b: number, c: number) => void;
  readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;
/**
* Instantiates the given `module`, which can either be bytes or
* a precompiled `WebAssembly.Module`.
*
* @param {SyncInitInput} module
*
* @returns {InitOutput}
*/
export function initSync(module: SyncInitInput): InitOutput;

/**
* If `module_or_path` is {RequestInfo} or {URL}, makes a request and
* for everything else, calls `WebAssembly.instantiate` directly.
*
* @param {InitInput | Promise<InitInput>} module_or_path
*
* @returns {Promise<InitOutput>}
*/
export default function __wbg_init (module_or_path?: InitInput | Promise<InitInput>): Promise<InitOutput>;
