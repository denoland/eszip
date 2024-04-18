// deno-lint-ignore-file
// deno-fmt-ignore-file

export interface InstantiateResult {
  instance: WebAssembly.Instance;
  exports: {
    build: typeof build;
    Parser : typeof Parser 
  };
}

/** Gets if the Wasm module has been instantiated. */
export function isInstantiated(): boolean;

/** Options for instantiating a Wasm instance. */
export interface InstantiateOptions {
  /** Optional url to the Wasm file to instantiate. */
  url?: URL;
  /** Callback to decompress the raw Wasm file bytes before instantiating. */
  decompress?: (bytes: Uint8Array) => Uint8Array;
}

/** Instantiates an instance of the Wasm module returning its functions.
* @remarks It is safe to call this multiple times and once successfully
* loaded it will always return a reference to the same object. */
export function instantiate(opts?: InstantiateOptions): Promise<InstantiateResult["exports"]>;

/** Instantiates an instance of the Wasm module along with its exports.
 * @remarks It is safe to call this multiple times and once successfully
 * loaded it will always return a reference to the same object. */
export function instantiateWithInstance(opts?: InstantiateOptions): Promise<InstantiateResult>;

/**
* Serialize a module graph into eszip.
* @param {any} roots
* @param {Function} loader
* @param {any} import_map_url
* @returns {Promise<Uint8Array>}
*/
export function build(roots: any, loader: Function, import_map_url: any): Promise<Uint8Array>;
/**
*/
export class Parser {
  free(): void;
/**
*/
  constructor();
/**
* Parse from a BYOB readable stream.
* @param {ReadableStreamBYOBReader} stream
* @returns {Promise<any>}
*/
  parse(stream: ReadableStreamBYOBReader): Promise<any>;
/**
* Parse from an in-memory buffer.
* @param {Uint8Array} buffer
* @returns {Promise<any>}
*/
  parseBytes(buffer: Uint8Array): Promise<any>;
/**
* Load module sources.
* @returns {Promise<any>}
*/
  load(): Promise<any>;
/**
* Get a module source.
* @param {string} specifier
* @returns {Promise<any>}
*/
  getModuleSource(specifier: string): Promise<any>;
/**
* Get a module sourcemap.
* @param {string} specifier
* @returns {Promise<any>}
*/
  getModuleSourceMap(specifier: string): Promise<any>;
}
