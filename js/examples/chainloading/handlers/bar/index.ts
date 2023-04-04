import { add } from "./helpers.js";

export default async (req: Request) => {
  return new Response(`bar: ${req.url} [${add(1, 2)}]`);
};
