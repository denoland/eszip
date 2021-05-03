/// This module is deployed to eszip-tests.deno.dev and is used in the tests.

addEventListener("fetch", (e) => {
  e.respondWith(new Response(`"${e.request.headers.get("x-magic-auth")}"`));
});
