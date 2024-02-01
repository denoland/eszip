import { load, LoadResponse } from "./loader.ts";
import {
  instantiate,
  Parser as InternalParser,
} from "./eszip_wasm.generated.js";

const encoder = new TextEncoder();

export type { LoadResponse } from "./loader.ts";

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
  loader: (url: string) => Promise<LoadResponse | undefined> = load,
  importMapUrl?: string,
): Promise<Uint8Array> {
  const { build } = await instantiate({ url: options.wasmURL });
  return build(
    roots,
    (specifier: string) => {
      return loader(specifier).then(result => {
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
