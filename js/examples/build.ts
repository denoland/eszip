import { build } from "../mod.ts";

const path = new URL("./worker.tsx", import.meta.url).href;
console.log(path);
const eszip = await build([path]);
console.log(eszip);
