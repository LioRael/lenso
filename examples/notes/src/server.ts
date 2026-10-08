import { startApp } from "@lenso/core";
import { reportDevReady } from "@lenso/engine/dev-ready";
import { createBunListenerPlugin } from "@lenso/web/bun";
import { createPgNotesPlugins, databaseUrl } from "./app-pg";
import type { createNotesApplication } from "./application";
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
  return startNotesServer(definition, port);
}

export async function startNotesServer<T>(
  definition: ReturnType<typeof createNotesApplication<T>>,
  port: number,
) {
  const web = createNotesWebPlugin(definition.notes, definition.authentication);
  const listener = createBunListenerPlugin({
    id: "notes-listener",
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
  reportDevReady({
    urls: [server.url],
    capabilities: server.app.status().map((plugin) => plugin.id),
  });
  const stop = () => {
    void server.app.stop().catch((error: unknown) => {
      console.error(error);
      process.exitCode = 1;
    });
  };
  process.once("SIGINT", stop);
  process.once("SIGTERM", stop);
}
