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
cargo run --example eszip_run file_server.eszip2 https://deno.land/std/http/file_server.ts
```
