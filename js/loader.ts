import { createCache } from "https://deno.land/x/deno_cache@0.7.1/mod.ts";
export type {
  Loader,
  LoadResponse,
  LoadResponseExternal,
  LoadResponseModule,
} from "https://deno.land/x/deno_cache@0.7.1/mod.ts";

const cache = createCache();
export const load = cache.load;
