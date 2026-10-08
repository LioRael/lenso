import { expect, test } from "bun:test";
import { createExampleServer } from "./server";

test("Greeting listener retains loopback host and exact listener-origin policy", async () => {
  const server = await createExampleServer(0);
  const port = Number(server.url.port);
  try {
    const accepted = await fetch(server.url, { headers: { origin: server.url.origin } });
    expect(accepted.status).not.toBe(403);
    await accepted.arrayBuffer();
    const host = await fetch(server.url, { headers: { host: "untrusted.test" } });
    expect(host.status).toBe(403);
    expect(await host.text()).toBe("Invalid host");
    const origin = await fetch(server.url, {
      headers: { origin: `http://localhost:${port}` },
    });
    expect(origin.status).toBe(403);
    expect(await origin.text()).toBe("Invalid origin");
    const absent = await fetch(server.url);
    expect(absent.status).not.toBe(403);
    await absent.arrayBuffer();
  } finally {
    const stopped = server.app.stop();
    expect(server.app.stop()).toBe(stopped);
    await stopped;
  }
  const replacement = Bun.serve({
    hostname: "127.0.0.1",
    port,
    fetch: () => new Response("released"),
  });
  await replacement.stop(true);
});
