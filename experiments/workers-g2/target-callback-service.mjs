// A controlled Workerd service binding used only to exercise the Host callback
// lifecycle. It is explicitly not a PostgreSQL or Hyperdrive implementation.
export default {
  async fetch(request) {
    const url = new URL(request.url);
    if (request.method !== "POST") return new Response("method", { status: 405 });
    if (url.pathname === "/fail") {
      await request.text();
      return new Response("backend callback failure", { status: 503 });
    }
    if (url.pathname === "/delay") {
      await request.text();
      await new Promise((resolve) => setTimeout(resolve, 25));
      return new Response('{"outcome":"created"}');
    }
    return new Response("not found", { status: 404 });
  },
};
