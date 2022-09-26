import { load, LoadResponse } from "./loader.ts";
import {
  instantiate,
  Parser as InternalParser,
} from "./eszip_wasm.generated.js";

export type { LoadResponse } from "./loader.ts";

export class EszipError extends Error {
  name = "EszipError";

  specifier?: string;
  line?: number;
  column?: number;

  constructor(
    message: string,
    specifier?: string,
    line?: number,
    column?: number,
  ) {
    super(message);
    this.specifier = specifier;
    this.line = line;
    this.column = column;
  }
}

export async function build(
  roots: string[],
  loader: (url: string) => Promise<LoadResponse | undefined> = load,
  importMapUrl?: string,
): Promise<Uint8Array> {
  const { build } = await instantiate();
  try {
    return await build(
      roots,
      (specifier: string) =>
        loader(specifier).catch((err) => Promise.reject(String(err))),
      importMapUrl,
    );
  } catch (e) {
    if (!(e instanceof Error) && e.message) {
      throw new EszipError(e.message, e.specifier, e.line, e.column);
    }
    throw e;
  }
}
