import { Parser, build } from "./mod.ts";
import { assert, assertEquals } from "https://deno.land/std@0.123.0/testing/asserts.ts";

Deno.test("roudtrip build + parse", async () => {
  const eszip = await build([
    "https://example.com/mod.ts",
    "https://example.com/a.ts",
  ], (specifier: string) => {
    if (specifier === "https://example.com/a.ts") {
      return {
        specifier,
        headers: {
          "content-type": "text/typescript",
        },
        content: "export const a = 1;",
      }
    };

    return {
      specifier: "https://example.com/mod.ts",
      headers: {
        "content-type": "application/typescript",
      },
      content: `import "https://example.com/a.ts";`,
    };
  });
  
  assert(eszip instanceof Uint8Array);
  const parser = new Parser();
  const specifiers = await parser.parseBytes(eszip);
  assertEquals(specifiers, [
    "https://example.com/mod.ts",
    "https://example.com/a.ts",
  ]);
  
  await parser.load();
  const mod = await parser.getModuleSource("https://example.com/mod.ts");
  assertEquals(mod, "import \"https://example.com/a.ts\";\n");
  const a = await parser.getModuleSource("https://example.com/a.ts");
  assertEquals(a, "export const a = 1;\n");
});

