
To create a new archieve:
```
> estar new kament.tar https://raw.githubusercontent.com/satyarohith/kament/main/mod.ts
Download https://raw.githubusercontent.com/satyarohith/kament/main/mod.ts
...
Wrote 256 KB -> kament.tar
```

To print a module from an archive to stdout:
```
> estar read kament.tar https://raw.githubusercontent.com/satyarohith/kament/main/mod.ts
/**
 * Kament Server Module
 *
 * This script exports the routes of Kament API along with their corresponding handlers.
 */
import { tokenHandler } from "./api/token.ts";
import { commentsHandler } from "./api/comments.ts";

const kamentApiRoutes = {
  "/api/token": tokenHandler,
  "/api/comments/:postslug": commentsHandler,
};

export { commentsHandler, kamentApiRoutes, tokenHandler };
```
