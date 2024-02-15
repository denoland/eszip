import type { Loader } from "./loader.ts";
import {
  instantiate,
  Parser as InternalParser,
} from "./eszip_wasm.generated.js";
import {
  CacheSetting,
  createCache,
} from "https://deno.land/x/deno_cache@0.7.1/mod.ts";
export type { LoadResponse } from "./loader.ts";

const encoder = new TextEncoder();

export const options: { wasmURL: URL | undefined } = { wasmURL: undefined };

export class Parser extends InternalParser {
  private constructor() {
    super();
  }

  static async createInstance() {
    // insure instantiate is called
    await instantiate({ url: options.wasmURL });
    return new Parser();
  }
}

export async function build(
  roots: string[],
  loader: Loader["load"] = createCache().load,
  importMapUrl?: string,
): Promise<Uint8Array> {
  const { build } = await instantiate({ url: options.wasmURL });
  return build(
    roots,
    (specifier: string, options: {
      isDynamic: boolean;
      cacheSetting: CacheSetting;
      checksum: string | undefined;
    }) => {
      return loader(
        specifier,
        options.isDynamic,
        options.cacheSetting,
        options.checksum,
      ).then((result) => {
        if (result?.kind === "module") {
          if (typeof result.content === "string") {
            result.content = encoder.encode(result.content);
          }
          // need to convert to an array for serde_wasm_bindgen to work
          // deno-lint-ignore no-explicit-any
          (result as any).content = Array.from(result.content);
        }
        return result;
      }).catch((err) => Promise.reject(String(err)));
    },
    importMapUrl,
  );
}
