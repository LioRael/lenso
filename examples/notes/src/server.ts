import { createWebPlugin } from "@lenso/web";
import { definePlugin, startApp } from "lenso";
import { createPgNotesPlugins, databaseUrl } from "./app-pg";
import { createNotesRouter } from "./router";

export async function createNotesServer(connection: string, port = 3001) {
  const definition = createPgNotesPlugins(connection);
  const web = createWebPlugin({
    id: "notes-web",
    requires: [definition.notes],
    router: (context) => createNotesRouter(context.get(definition.notes)),
  });
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
  return { app, notes: app.get(definition.notes), url: app.get(listener).url };
}

if (import.meta.main) {
  const server = await createNotesServer(databaseUrl(), Number(process.env.LENSO_PORT ?? 3001));
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
