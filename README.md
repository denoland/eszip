# eszip

A utility that can download JavaScript and TypeScript module graphs and store
them locally in a special zip file.

To create a new archive:
```
> eszip get https://raw.githubusercontent.com/satyarohith/kament/main/mod.ts
Download https://raw.githubusercontent.com/satyarohith/kament/main/mod.ts
...
Wrote es.zip
```

To print the list of modules in an eszip file:
```
> eszip list es.zip
https://denopkg.com/chiefbiiko/sha512/mod.ts
https://deno.land/x/djwt@v2.1/algorithm.ts
https://deno.land/x/god_crypto@v1.4.8/src/rsa/rsa_key.ts
https://deno.land/x/god_crypto@v1.4.8/src/rsa/rsa_js.ts
...
```

To read a module from the archive:
```
> eszip read es.zip https://denopkg.com/chiefbiiko/sha512/mod.ts
import { encode, decode } from "./deps.ts";

/** Byte length of a SHA512 hash. */
export const BYTES: number = 64;

/** A class representation of the SHA2-512 algorithm. */
export class SHA512 {
  readonly hashSize: number = BYTES;
...
```
