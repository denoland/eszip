export default async (req: Request) => {
  return new Response(`foo: ${req.url}`);
};
