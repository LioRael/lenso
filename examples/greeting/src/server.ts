import { definePlugin, startApp, type RunningApp } from "@lenso/core";
import { reportDevReady } from "@lenso/engine/dev-ready";
import { createBunListenerPlugin } from "@lenso/web/bun";
import { greeting } from "./greeting";
import { createGreetingWeb } from "./web";

export async function createExampleServer(port = 3000) {
  let running: RunningApp | undefined;
  const web = createGreetingWeb(() => running?.status() ?? []);
  const bunListener = createBunListenerPlugin({
    id: "http-listener",
    web,
    hostname: "127.0.0.1",
    port,
    ingress(request, url) {
      const host = new URL(request.url).hostname;
      if (host !== "127.0.0.1" && host !== "localhost")
        return new Response("Invalid host", { status: 403 });
      const origin = request.headers.get("origin");
      if (origin && origin !== url.origin) return new Response("Invalid origin", { status: 403 });
    },
  });
  const listener = definePlugin({
    ...bunListener,
    async setup(context) {
      let address: { port: number } | undefined;
      context.onCleanup(() => {
        if (address) console.log(`[example] closed pid=${process.pid} port=${address.port}`);
      });
      const service = await bunListener.setup(context);
      address = service;
      return service;
    },
  });
  running = await startApp({ plugins: [greeting, web, listener] });
  return { app: running, url: running.get(listener).url };
}

if (import.meta.main) {
  const example = await createExampleServer(Number(process.env.LENSO_PORT ?? 3000));
  reportDevReady({
    urls: [example.url],
    capabilities: example.app.status().map((plugin) => plugin.id),
  });
  let stopping: Promise<void> | undefined;
  const shutdown = () => {
    stopping ??= example.app
      .stop()
      .then(() => {
        process.exitCode = 0;
      })
      .catch((error) => {
        console.error(error);
        process.exitCode = 1;
      });
  };
  process.once("SIGTERM", shutdown);
  process.once("SIGINT", shutdown);
}
