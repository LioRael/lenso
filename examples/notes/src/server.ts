import { definePlugin, startApp } from "@lenso/core";
import { createPgNotesPlugins, databaseUrl } from "./app-pg";
import { parseNotesPrincipals, type NotesPrincipal } from "./auth";
import type { SessionLifetime } from "@lenso/auth/sessions";
import { createNotesWebPlugin } from "./web";

export async function createNotesServer(
  connection: string,
  principals: readonly NotesPrincipal[],
  port = 3001,
  lifetime?: SessionLifetime,
) {
  const definition = createPgNotesPlugins(connection, principals, lifetime);
  const web = createNotesWebPlugin(definition.notes, definition.authentication);
  const listener = definePlugin({
    id: "notes-listener",
    requires: [web],
    setup(context) {
      const handler = context.get(web);
      const server = Bun.serve({
        hostname: "127.0.0.1",
        port,
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
  });
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
    parseNotesPrincipals(process.env.NOTES_LOGIN_KEYS),
    Number(process.env.LENSO_PORT ?? 3001),
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
