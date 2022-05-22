# eszip

The eszip format lets you losslessly serialize an ECMAScript module graph
(represented by [`deno_graph::ModuleGraph`][module_graph]) into a single compact
file.

The eszip file format is designed to be compact and streaming capable. This
allows for efficient loading of large ECMAScript module graphs.

[module_graph]: https://docs.rs/deno_graph/latest/deno_graph/struct.ModuleGraph.html

## Examples

### Creating an eszip

```shell
cargo run --example eszip_builder https://deno.land/std/http/file_server.ts file_server.eszip2
```

### Viewing the contents of an eszip

```shell
cargo run --example eszip_viewer file_server.eszip2
```

### Loading the eszip into V8

```shell
cargo run --example eszip_load file_server.eszip2 https://deno.land/std/http/file_server.ts
```

## File format

The file format looks as follows:

```
Eszip:
| Magic (8) | Header size (4) | Header (n) | Header hash (32) | Sources size (4) | Sources (n) | SourceMaps size (4) | SourceMaps (n) |

Header:
( | Specifier size (4) | Specifier (n) | Entry type (1) | Entry (n) | )*

Entry (redirect):
| Specifier size (4) | Specifier (n) |

Entry (module):
| Source offset (4) | Source size (4) | SourceMap offset (4) | SourceMap size (4) | Module type (1) |

Sources:
( | Source (n) | Hash (32) | )*

SourceMaps:
( | SourceMap (n) | Hash (32) | )*
```

There is one optimization for empty source / source map entries. If both the
offset and size are set to 0, no entry and no hash is present in the data
sections for that module.

## Development

```
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli
brew install binaryen
```

When opening a PR make sure to rebuild WASM by running:

```
deno task release
```

### Troubleshooting

Errors like:

```
thread 'main' panicked at 'remaining data [20, 10, 85, 105, 110, 116, 56, 65, 114, 114, 97, 121]', /.cargo/registry/src/github.com-1ecc6299db9ec823/wasm-bindgen-cli-support-0.2.78/src/descriptor.rs:111:9
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
wasm-bindgen failed
make: *** [build] Error 1
```

mean that `wasm-bindgen-cli` doesn't match version specified in `Cargo.toml`.

To fix it, run `cargo install wasm-bindgen-cli --version VERSION` where
`VERSION` matches what's specified in `Cargo.toml`.
