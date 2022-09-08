import { build, Parser } from "./mod.ts";
import {
  assert,
  assertEquals,
  assertRejects,
} from "https://deno.land/std@0.123.0/testing/asserts.ts";

Deno.test("roundtrip build + parse", async () => {
  const eszip = await build([
    "https://example.com/mod.ts",
    "https://example.com/a.ts",
    "external:main.js",
  ], async (specifier: string) => {
    if (specifier === "external:main.js") {
      return {
        kind: "external",
        specifier,
      };
    }

    if (specifier === "https://example.com/a.ts") {
      return {
        kind: "module",
        specifier,
        headers: {
          "content-type": "text/typescript",
        },
        content: "export const a = 1;",
      };
    }

    return {
      kind: "module",
      specifier: "https://example.com/mod.ts",
      headers: {
        "content-type": "application/typescript",
      },
      content: `import "https://example.com/a.ts";`,
    };
  });

  assert(eszip instanceof Uint8Array);
  const parser = await Parser.createInstance();
  const specifiers = await parser.parseBytes(eszip);
  assertEquals(specifiers.sort(), [
    "https://example.com/a.ts",
    "https://example.com/mod.ts",
  ]);

  await parser.load();
  const mod = await parser.getModuleSource("https://example.com/mod.ts");
  assertEquals(mod, 'import "https://example.com/a.ts";\n');
  const a = await parser.getModuleSource("https://example.com/a.ts");
  assertEquals(a, "export const a = 1;\n");
});

Deno.test("build default loader", async () => {
  const eszip = await build(["https://deno.land/std@0.123.0/fs/mod.ts"]);
  assert(eszip instanceof Uint8Array);
});

Deno.test("build with import map", async () => {
  const eszip = await build(["data:application/javascript,import 'std/fs/mod.ts'"], undefined, "data:application/json,{\"imports\":{\"std/\":\"https://deno.land/std/\"}}");
  assert(eszip instanceof Uint8Array);
});

Deno.test("loader errors", async () => {
  await assertRejects(
    () =>
      build(
        ["https://deno.land/std@0.123.0/fs/mod.ts"],
        (specifier: string) => Promise.reject(new Error("oops")),
      ),
    undefined,
    "oops",
  );
});
