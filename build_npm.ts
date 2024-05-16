import { build, emptyDir } from "jsr:@deno/dnt@0.41.1";

await emptyDir("./npm");
Deno.mkdirSync("npm/esm", { recursive: true });
Deno.mkdirSync("npm/script");

Deno.copyFileSync("js/eszip_wasm_bg.wasm", "npm/esm/eszip_wasm_bg.wasm");
// todo(dsherret): how to not include two copies of this in the npm
// package? Does using a symlink work?
Deno.copyFileSync("js/eszip_wasm_bg.wasm", "npm/script/eszip_wasm_bg.wasm");

await build({
  entryPoints: ["./js/mod.ts"],
  outDir: "./npm",
  shims: {
    deno: true,
    undici: true,
  },
  scriptModule: false,
  package: {
    name: "@deno/eszip",
    version: Deno.args[0],
    description:
      "A utility that can download JavaScript and TypeScript module graphs and store them locally in a special zip file",
    license: "MIT",
    repository: {
      type: "git",
      url: "git+https://github.com/denoland/eszip.git",
    },
    bugs: {
      url: "https://github.com/denoland/eszip/issues",
    },
  },
  compilerOptions: {
    lib: ["DOM", "ES2021"],
  },
  postBuild() {
    addWebCryptoGlobal("npm/esm/mod.js");
  },
});

function addWebCryptoGlobal(filePath: string) {
  const fileText = Deno.readTextFileSync(filePath);
  // https://docs.rs/getrandom/latest/getrandom/#nodejs-es-module-support
  Deno.writeTextFileSync(
    filePath,
    `import { webcrypto } from 'node:crypto';\nif (!globalThis.crypto) {\n  globalThis.crypto = webcrypto;\n}\n` +
      fileText,
  );
}

Deno.copyFileSync("LICENSE", "npm/LICENSE");
Deno.copyFileSync("README.md", "npm/README.md");
