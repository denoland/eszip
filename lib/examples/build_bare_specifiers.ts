import { build, LoadResponse } from "../mod.ts";
import { load } from "../loader.ts";

const BASE_URL = "file:///main.js";
// Bundle a new eszip.
const eszip = await build([
  "eszip:main",
], async (specifier: string): Promise<LoadResponse | undefined> => {
  if (specifier == "eszip:main") {
    return {
      kind: "module",
      specifier,
      headers: {
        "content-type": "text/typescript",
      },
      content: `import "./worker.tsx"`,
    };
  }

  return load(specifier);
}, (specifier: string, referrer: string) => {
  if (referrer == "eszip:main") return new URL(specifier, import.meta.url).href;
  return undefined;
});

console.log(eszip);
