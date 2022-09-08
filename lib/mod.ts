import { load, LoadResponse } from "./loader.ts";
import {
  instantiate,
  Parser as InternalParser,
} from "./eszip_wasm.generated.js";

export type { LoadResponse } from "./loader.ts";

export class Parser extends InternalParser {
  private constructor() {
    super();
  }

  static async createInstance() {
    // insure instantiate is called
    await instantiate();
    return new Parser();
  }
}

export async function build(
  roots: string[],
  loader: (url: string) => Promise<LoadResponse | undefined> = load,
  importMapUrl?: string,
): Promise<Uint8Array> {
  const { build } = await instantiate();
  return build(
    roots,
    (specifier: string) =>
      loader(specifier).catch((err) => Promise.reject(String(err))),
    importMapUrl,
  );
}
