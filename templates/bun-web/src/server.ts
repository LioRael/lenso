import { startApp } from "@lenso/core";
import { bindConfig, definePluginConfig, type ConfigSource } from "@lenso/core/config";
import { envSource } from "@lenso/core/config/env";
import { createWebPlugin } from "@lenso/web";
import { createBunListenerPlugin } from "@lenso/web/bun";
import { reportDevReady } from "@lenso/engine/dev-ready";
import { diagnostic } from "@lenso/engine/diagnostics";
import { z } from "zod";
import { createGreetingPlugin } from "./greeting";
import { createRouter } from "./router";

export { greetingInput, createGreetingService } from "./greeting";
export type { AppRouter, AppClient } from "./router";

export const greeting = createGreetingPlugin([
  envSource({
    id: "greeting-env",
    read: (name) => process.env[name],
    bindings: { prefix: { name: "GREETING_PREFIX" } },
  }),
]);

const listenerConfig = definePluginConfig({
  schema: z.strictObject({ port: z.number().int().min(0).max(65535).default(3000) }),
});

export async function startServer(port?: number | readonly ConfigSource[]) {
  const web = createWebPlugin({
    requires: [greeting],
    router: (context) => createRouter(context.get(greeting)),
  });
  const listener = bindConfig(
    listenerConfig,
    typeof port === "number" || port === undefined ? { port } : port,
    {
      id: "http",
      requires: [web],
      setup(context, config) {
        return createBunListenerPlugin({
          id: "http",
          web,
          hostname: "127.0.0.1",
          port: config.port,
          ingress: () => undefined,
        }).setup(context);
      },
    },
  );
  const app = await startApp({ plugins: [greeting, web, listener] });
  return { app, url: app.get(listener).url };
}

if (import.meta.main) {
  try {
    const { app, url } = await startServer([
      envSource({
        id: "listener-env",
        read: (name) => process.env[name],
        bindings: { port: { name: "LENSO_PORT", type: "number" } },
      }),
    ]);
    console.log(`Ready at ${url}`);
    reportDevReady({ urls: [url], capabilities: ["greeting.greet via Web/oRPC"] });
    const stop = () => {
      void app.stop().catch((error: unknown) => {
        console.error(JSON.stringify(diagnostic(error)));
        process.exitCode = 1;
      });
    };
    process.once("SIGINT", stop);
    process.once("SIGTERM", stop);
  } catch (error) {
    console.error(JSON.stringify(diagnostic(error)));
    process.exitCode = 1;
  }
}
