import { startApp } from "@lenso/core";
import { bindConfig, definePluginConfig, type ConfigSource } from "@lenso/core/config";
import { envSource } from "@lenso/core/config/env";
import { createPgNotesPlugins, databaseUrl } from "./app-pg";
import type { NotesPrincipalsInput } from "./auth";
import type { SessionLifetime } from "@lenso/auth/sessions";
import { createNotesWebPlugin } from "./web";
import { z } from "zod";

export const notesListenerConfig = definePluginConfig({
  schema: z.strictObject({ port: z.number().int().min(0).max(65535).default(3001) }),
});

export async function createNotesServer(
  connection: string,
  principals: NotesPrincipalsInput,
  port?: number | readonly ConfigSource[],
  lifetime?: SessionLifetime,
) {
  const definition = createPgNotesPlugins(connection, principals, lifetime);
  const web = createNotesWebPlugin(definition.notes, definition.authentication);
  const listener = bindConfig(
    notesListenerConfig,
    typeof port === "number" || port === undefined ? { port } : port,
    {
      id: "notes-listener",
      requires: [web],
      setup(context, config) {
        const handler = context.get(web);
        const server = Bun.serve({
          hostname: "127.0.0.1",
          port: config.port,
          fetch(request) {
            const host = new URL(request.url).hostname;
            if (host !== "127.0.0.1" && host !== "localhost")
              return new Response("Invalid host", { status: 403 });
            const origin = request.headers.get("origin");
            if (origin && origin !== server.url.origin)
              return new Response("Invalid origin", { status: 403 });
            return handler.fetch(request);
          },
        });
        context.onCleanup(() => server.stop(true));
        return { url: server.url };
      },
    },
  );
  const app = await startApp({ plugins: [...definition.plugins, web, listener] });
  return {
    app,
    notes: app.get(definition.notes),
    authentication: app.get(definition.authentication),
    url: app.get(listener).url,
  };
}

if (import.meta.main) {
  const server = await createNotesServer(
    databaseUrl(),
    {
      sources: [
        envSource({
          id: "notes-env",
          read: (name) => process.env[name],
          bindings: { principals: { name: "NOTES_LOGIN_KEYS", sensitive: true } },
        }),
      ],
    },
    [
      envSource({
        id: "notes-listener-env",
        read: (name) => process.env[name],
        bindings: { port: { name: "LENSO_PORT", type: "number" } },
      }),
    ],
  );
  console.log(`Notes RPC ready at ${server.url}rpc`);
  const stop = () => {
    void server.app.stop().catch((error: unknown) => {
      console.error(error);
      process.exitCode = 1;
    });
  };
  process.once("SIGINT", stop);
  process.once("SIGTERM", stop);
}
