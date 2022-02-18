import { load, LoadResponse } from "./loader.ts";
import { build as _build } from "./eszip_wasm.generated.js";

export { Parser } from "./eszip_wasm.generated.js";
export type { LoadResponse } from "./loader.ts";

export function build(
  roots: string[],
  loader: (url: string) => Promise<LoadResponse | undefined> = load,
): Promise<Uint8Array> {
  return _build(
    roots,
    (specifier: string) =>
      loader(specifier).catch((err) => Promise.reject(String(err))),
  );
}
