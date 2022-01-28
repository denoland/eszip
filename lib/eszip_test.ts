import { Parser } from "./lib/eszip_wasm.generated.js";

const res = await fetch("http://localhost:8000/redirect.eszip2");
const reader = res.body!.getReader({ mode: "byob" });
const parser = new Parser();
console.log(await parser.parse(reader));

console.log(await parser.get_module_source("file:///main.ts"));

