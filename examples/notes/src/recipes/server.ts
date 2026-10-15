import { startApp } from "@lenso/core";
import { bindConfig, definePluginConfig } from "@lenso/core/config";
import { envSource } from "@lenso/core/config/env";
import { diagnostic } from "@lenso/engine/diagnostics";
import { reportDevReady } from "@lenso/engine/dev-ready";
import { createBunListenerPlugin, type BunListenerService } from "@lenso/web/bun";
import { z } from "zod";
import definition, { recipe } from "../../recipes.config";

const listenerConfig = definePluginConfig({
  schema: z.object({ port: z.coerce.number().int().min(0).max(65535).default(3000) }),
});
const listener = bindConfig(
  listenerConfig,
  [
    envSource({
      id: "recipe-listener-env",
      read: (name) => process.env[name],
      bindings: { port: { name: "PORT", type: "number" } },
    }),
  ],
  {
    id: "recipe-listener",
    requires: [recipe.web],
    async setup(context, { port }): Promise<BunListenerService> {
      return createBunListenerPlugin({
        id: "recipe-listener",
        web: recipe.web,
        hostname: "127.0.0.1",
        port,
        ingress(request, url) {
          const host = new URL(request.url).hostname;
          if (host !== "127.0.0.1" && host !== "localhost")
            return new Response(null, { status: 403 });
          const origin = request.headers.get("origin");
          if (origin && origin !== url.origin) return new Response(null, { status: 403 });
        },
      }).setup(context);
    },
  },
);

if (import.meta.main)
  try {
    const app = await startApp({ ...definition, plugins: [...definition.plugins, listener] });
    console.log(`Recipes listening at ${app.get(listener).url}`);
    reportDevReady({
      urls: [app.get(listener).url],
      capabilities: app.status().map(({ id }) => id),
    });
    let stopping = false;
    const stop = async () => {
      if (stopping) return;
      stopping = true;
      try {
        await app.stop();
      } catch (error) {
        console.error(JSON.stringify(diagnostic(error)));
        process.exitCode = 1;
      }
    };
    process.once("SIGINT", stop);
    process.once("SIGTERM", stop);
  } catch (error) {
    console.error(JSON.stringify(diagnostic(error)));
    process.exitCode = 1;
  }
