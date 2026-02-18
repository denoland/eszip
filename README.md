# eszip

The eszip format lets you losslessly serialize an ECMAScript module graph into a
single compact file.

The eszip file format is designed to be compact and streaming capable. This
allows for efficient loading of large ECMAScript module graphs.

https://eszip-viewer.deno.dev/ is a tool for inspecting eszip files.

> **Note:** The Rust `eszip` crate source code has been moved to the
> [denoland/deno](https://github.com/denoland/deno) repository. This repository
> contains the JavaScript/WebAssembly bindings that wrap the
> [eszip crate](https://crates.io/crates/eszip).

## Development

Build the Wasm module:

```
deno task build
```

Run tests:

```
deno task test
```
