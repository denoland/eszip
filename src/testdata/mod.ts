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
