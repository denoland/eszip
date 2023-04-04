#!/usr/bin/env -S deno run --allow-read --allow-write --allow-net --no-check
import { load } from "../../loader.ts";
import { build, LoadResponse } from "../../mod.ts";
// import { load } from "https://deno.land/x/eszip@v0.18.0/loader.ts";
// import { build, LoadResponse } from "https://deno.land/x/eszip@v0.18.0/mod.ts";

import * as path from "https://deno.land/std@0.127.0/path/mod.ts";

const STAGE1_SPECIFIER = "acme:stage1";
const STAGE2_SPECIFIER = "acme:stage2";

function stage1() {
  return `
    import * as stage2 from "${STAGE2_SPECIFIER}";
    import { serve } from "https://deno.land/std@0.127.0/http/server.ts";

    await serve(async (req) => {
      const url = new URL(req.url);
      const handler = stage2.config.urls[url.pathname];
      const handlerFn = stage2.handlers[handler];
      return handlerFn ? await handlerFn(req) : new Response("DEFAULT");
    });
  `;
}

function inlineTsMod(specifier: string, content: string): LoadResponse {
  return {
    content,
    headers: {
      "content-type": "application/typescript",
    },
    kind: "module",
    specifier,
  };
}

function stage1Loader() {
  return async (specifier: string): Promise<LoadResponse | undefined> => {
    if (specifier === STAGE1_SPECIFIER) {
      // Load stage 1 wrapper from net/disk or codegenerated in-memory
      return inlineTsMod(specifier, stage1());
    } else if (specifier === STAGE2_SPECIFIER) {
      // Dangling reference to stage2
      return { kind: "external", specifier };
    }
    // Falling back to the default loading logic.
    return load(specifier);
  };
}

interface Stage2Config {
  paths: Record<string, string>; // handler_name => entrypoint_path (absolute)
  urls: Record<string, string>; // url => handler_name mapping
  localSrcRoot: string;
}

function stage2(conf: Stage2Config) {
  const handlerNames = Object.keys(conf.paths);
  const handlerImports = handlerNames.map((name) => {
    return `import ${name} from "${nsSpec(conf, name)}";`;
  }).join("\n");
  return `
    ${handlerImports}
    export const handlers = { ${handlerNames.join(", ")} };
    export const config = ${JSON.stringify(conf)};
  `;
}

/// Namespaced specifier, file:///src/... (relative to root)
function nsSpec(conf: Stage2Config, name: string) {
  const filepath = conf.paths[name];
  const rel = path.relative(conf.localSrcRoot, filepath);
  return path.toFileUrl(path.join("/src", rel));
}

function stage2Loader(conf: Stage2Config) {
  return async (specifier: string): Promise<LoadResponse | undefined> => {
    if (specifier === STAGE2_SPECIFIER) {
      // Codegen stage2 from config to include handler specifics
      return inlineTsMod(specifier, stage2(conf));
    } else if (specifier.startsWith("file:///src/")) {
      // Local specifier (handler entrypoint or relative import)
      const localRoot = path.toFileUrl(conf.localSrcRoot).toString();
      const trueSpec = specifier.replace("file:///src", localRoot);
      const resp = await load(trueSpec);
      return resp && {
        ...resp,
        specifier, // preserve original spec
      };
    }
    // Falling back to the default loading logic.
    return await load(specifier);
  };
}

function unifiedLoader(conf: Stage2Config) {
  const s2Loader = stage2Loader(conf);
  return async (specifier: string): Promise<LoadResponse | undefined> => {
    if (specifier === STAGE1_SPECIFIER) {
      return inlineTsMod(specifier, stage1());
    }
    return s2Loader(specifier);
  };
}

async function main(allArgs: string[]) {
  // Specifics of how you obtain the list of handlers is up to you
  // e.g: conf/project files, FS walking, etc...
  const cwd = Deno.cwd();
  const stage2Config: Stage2Config = {
    paths: {
      foo: path.join(cwd, "handlers", "foo.ts"),
      bar: path.join(cwd, "handlers", "bar", "index.ts"),
    },
    urls: {
      "/foo": "foo",
      "/kungfu": "foo",
      "/bar": "bar",
      "/drink": "bar",
    },
    localSrcRoot: cwd,
  };

  const [cmd, ...args] = allArgs;
  switch (cmd) {
    // Produces an ESZIP with only stage1
    case "stage1": {
      const [destPath] = args;
      const bytes = await build([STAGE1_SPECIFIER], stage1Loader());
      return await Deno.writeFile(destPath, bytes);
    }
    // Produces an ESZIP with only stage2
    case "stage2": {
      const [destPath] = args;
      const bytes = await build([STAGE2_SPECIFIER], stage2Loader(stage2Config));
      return await Deno.writeFile(destPath, bytes);
    }
    // Produces an ESZIP with stage1 & stage2
    case "unified": {
      const [destPath] = args;
      const bytes = await build(
        [STAGE1_SPECIFIER],
        unifiedLoader(stage2Config),
      );
      return await Deno.writeFile(destPath, bytes);
    }
  }
}
await main(Deno.args);
