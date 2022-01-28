import { Parser } from "./lib/eszip_wasm.generated.js";

const res = await fetch("http://localhost:8000/redirect.eszip2");
const reader = res.body!.getReader({ mode: "byob" });

const parser = new Parser();
const specifiers = await parser.parse(reader);
const futures = [];

for (const specifier of specifiers) {
  futures.push(parser.getModuleSource(specifier));
}

await Promise.all([
  parser.load(),
  ...futures,
]);
