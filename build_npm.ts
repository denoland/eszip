import { build, emptyDir } from "https://deno.land/x/dnt@0.37.0/mod.ts";

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
    lib: ["dom", "es2021"],
  },
  filterDiagnostic(diagnostic) {
    if (
      diagnostic.file?.fileName.endsWith("wasmbuild@0.14.1/loader.ts")
    ) {
      return false;
    }
    return true;
  },
});

Deno.copyFileSync("LICENSE", "npm/LICENSE");
Deno.copyFileSync("README.md", "npm/README.md");
