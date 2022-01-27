import { eszip_parse } from './lib/eszip_wasm.generated.js';
// import { readableStreamFromReader } from "https://deno.land/std@0.123.0/streams/mod.ts";

function isCloser(value: unknown): value is Deno.Closer {
  return typeof value === "object" && value != null && "close" in value &&
    // deno-lint-ignore no-explicit-any
    typeof (value as Record<string, any>)["close"] === "function";
}

export function readableStreamFromReader(
  reader: Deno.Reader | (Deno.Reader & Deno.Closer),
): ReadableStream<Uint8Array> {
  return new ReadableStream({
    type: "bytes",
    async pull(controller) {
      const chunk = new Uint8Array(16_640);
      try {
        const read = await reader.read(chunk);
        if (read === null) {
          if (isCloser(reader)) {
            reader.close();
          }
          controller.close();
          return;
        }
        controller.enqueue(chunk.subarray(0, read));
      } catch (e) {
        controller.error(e);
        if (isCloser(reader)) {
          reader.close();
        }
      }
    },
    cancel() {
      if (isCloser(reader)) {
        reader.close();
      }
    },
  });
}

Deno.test("eszip_parse basic", async () => {
  const fd = await Deno.open("../src/testdata/redirect.eszip2", { read: true });
  const reader = readableStreamFromReader(fd).getReader({ mode: "byob" });

  await eszip_parse(reader);
});

