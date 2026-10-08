import { definePlugin, startApp, type RunningApp } from "lenso";
import { greeting } from "./greeting";
import { createGreetingWeb } from "./web";

export async function createExampleServer(port = 3000) {
  let running: RunningApp | undefined;
  const web = createGreetingWeb(() => running?.status() ?? []);
  const listener = definePlugin({
    id: "http-listener",
    requires: [web],
    setup(context) {
      const service = context.get(web);
      const server = Bun.serve({
        hostname: "127.0.0.1",
        port,
        fetch(request) {
          const host = new URL(request.url).hostname;
          if (host !== "127.0.0.1" && host !== "localhost")
            return new Response("Invalid host", { status: 403 });
          const origin = request.headers.get("origin");
          if (origin && origin !== server.url.origin) {
            return new Response("Invalid origin", { status: 403 });
          }
          return service.fetch(request);
        },
      });
      context.onCleanup(async () => {
        await server.stop(true);
        console.log(`[example] closed pid=${process.pid} port=${server.port}`);
      });
      return { url: server.url };
    },
  });
  running = await startApp({ plugins: [greeting, web, listener] });
  return { app: running, url: running.get(listener).url };
}

if (import.meta.main) {
  const example = await createExampleServer(Number(process.env.LENSO_PORT ?? 3000));
  console.log(`[example] ready pid=${process.pid} ${example.url}`);
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
