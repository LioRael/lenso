import { startApp } from "@lenso/core";
import { createWebPlugin } from "@lenso/web";
import { createBunListenerPlugin } from "@lenso/web/bun";
import { reportDevReady } from "@lenso/engine/dev-ready";
import { greeting } from "../src/application";
import { createRouter } from "../src/router";

export async function startServer(port = 3000) {
  const web = createWebPlugin({
    requires: [greeting],
    router: (context) => createRouter(context.get(greeting)),
  });
  const listener = createBunListenerPlugin({
    id: "http",
    web,
    hostname: "127.0.0.1",
    port,
    ingress: () => undefined,
  });
  const app = await startApp({ plugins: [greeting, web, listener] });
  return { app, url: app.get(listener).url };
}

if (import.meta.main) {
  const { app, url } = await startServer(Number(process.env.LENSO_PORT ?? 3000));
  console.log(`Ready at ${url}`);
  reportDevReady({ urls: [url], capabilities: ["greeting.greet via Web/oRPC"] });
  const stop = () => {
    void app.stop().catch(console.error);
  };
  process.once("SIGINT", stop);
  process.once("SIGTERM", stop);
}
