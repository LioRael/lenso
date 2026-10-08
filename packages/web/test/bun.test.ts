import { expect, test } from "bun:test";
import { defineApp, definePlugin, startApp } from "@lenso/core";
import { createBunListenerPlugin } from "../src/bun";
import type { WebService } from "../src/index";

test("Bun listener applies ingress policy then forwards to its exact Web instance", async () => {
  const first = definePlugin<WebService>({
    id: "first-web",
    setup: () => ({
      async fetch(request) {
        return new Response(`first:${new URL(request.url).pathname}`);
      },
    }),
  });
  const second = definePlugin<WebService>({
    id: "second-web",
    setup: () => ({ fetch: async () => new Response("wrong instance") }),
  });
  const web = definePlugin({
    id: "selected-web",
    requires: [first, second],
    setup: (context) => context.get(first),
  });
  const listener = createBunListenerPlugin({
    web,
    hostname: "127.0.0.1",
    port: 0,
    async ingress(request, url) {
      if (request.headers.get("origin") && request.headers.get("origin") !== url.origin)
        return new Response("Invalid origin", { status: 403 });
      if (new URL(request.url).pathname === "/blocked")
        return new Response(`denied:${request.method}`, { status: 403 });
      return undefined;
    },
  });
  const app = await startApp(defineApp({ plugins: [first, second, web, listener] }));
  const actualPort = app.get(listener).port;
  try {
    const service = app.get(listener);
    expect(service.port).toBeGreaterThan(0);
    expect(service.url.port).toBe(String(service.port));
    expect(await (await fetch(new URL("/blocked", service.url))).text()).toBe("denied:GET");
    const passed = await fetch(new URL("/allowed", service.url));
    expect(await passed.text()).toBe("first:/allowed");
    expect((await fetch(service.url, { headers: { origin: service.url.origin } })).status).toBe(
      200,
    );
    expect(
      (
        await fetch(service.url, {
          headers: { host: "untrusted.test", origin: "http://untrusted.test" },
        })
      ).status,
    ).toBe(403);
  } finally {
    await app.stop();
  }
  const probe = Bun.serve({
    hostname: "127.0.0.1",
    port: actualPort,
    fetch: () => new Response("released"),
  });
  await probe.stop(true);
});

test("listener startup rejects an undeclared Web instance", async () => {
  const web = definePlugin<WebService>({
    id: "web",
    setup: () => ({ fetch: async () => new Response() }),
  });
  const listener = createBunListenerPlugin({
    web,
    hostname: "127.0.0.1",
    port: 0,
    ingress: () => undefined,
  });
  const other = definePlugin({ id: "web", setup: () => ({}) });
  await expect(startApp(defineApp({ plugins: [listener, other] }))).rejects.toThrow(
    'requires missing instance "web"',
  );
});

test("startup rollback closes listener acquired before a later plugin fails", async () => {
  const web = definePlugin<WebService>({
    id: "web",
    setup: () => ({ fetch: async () => new Response("ok") }),
  });
  let port = 0;
  const listener = createBunListenerPlugin({
    web,
    hostname: "127.0.0.1",
    port: 0,
    ingress: () => undefined,
  });
  const capture = definePlugin({
    id: "capture",
    requires: [listener],
    setup(context) {
      port = context.get(listener).port;
    },
  });
  const failure = definePlugin({
    id: "failure",
    setup() {
      throw new Error("later setup failure");
    },
  });
  await expect(startApp(defineApp({ plugins: [web, listener, capture, failure] }))).rejects.toThrow(
    "later setup failure",
  );
  const probe = Bun.serve({ hostname: "127.0.0.1", port, fetch: () => new Response("ok") });
  await probe.stop(true);
});
