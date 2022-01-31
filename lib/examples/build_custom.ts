import { build } from "../mod.ts";

// Bundle a new eszip.
const eszip = await build([
  "https://example.com/mod.ts",
  "https://example.com/dep1.ts",
], async (specifier: string) => {
  if (specifier === "https://example.com/dep1.ts") {
    return {
      specifier,
      headers: {
        "content-type": "text/typescript",
      },
      content: "export const a = 1;",
    };
  }

  return {
    specifier: "https://example.com/mod.ts",
    headers: {
      "content-type": "application/typescript",
    },
    content: `import { a } from "https://example.com/dep1.ts";`,
  };
});

console.log(eszip);
