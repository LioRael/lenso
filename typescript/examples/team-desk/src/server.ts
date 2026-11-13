import { createServer } from "node:http";
import type { IncomingMessage } from "node:http";
import { mkdirSync } from "node:fs";
import { dirname } from "node:path";
import { pathToFileURL } from "node:url";
import { ActionError } from "@lenso/app";
import type { ErrorCode } from "@lenso/app";
import { createApplication } from "./app.js";
import { Store } from "./store.js";
import { identitySchema } from "./contracts.js";
import { randomUUID } from "node:crypto";

const status: Record<ErrorCode, number> = {
  invalid_input: 400, unauthenticated: 401, forbidden: 403, not_found: 404,
  conflict: 409, cancelled: 499, internal: 500,
};

async function jsonBody(request: IncomingMessage): Promise<unknown> {
  const chunks = [];
  let length = 0;
  for await (const chunk of request) {
    length += chunk.length;
    if (length > 1_500_000) throw new ActionError("invalid_input", "Request exceeds size limit");
    chunks.push(chunk);
  }
  try { return JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(Buffer.concat(chunks))); }
  catch (cause) { throw new ActionError("invalid_input", "Invalid JSON", { cause }); }
}

export function serve(store: Store) {
  const app = createApplication(store);
  return createServer(async (request, response) => {
    const controller = new AbortController();
    request.on("aborted", () => controller.abort());
    response.on("close", () => { if (!response.writableFinished) controller.abort(); });
    try {
      const path = new URL(request.url ?? "/", "http://localhost").pathname;
      const authorization = request.headers.authorization;
      const token = authorization?.startsWith("Bearer ") ? authorization.slice(7) : undefined;
      const context = store.context(token, randomUUID(), controller.signal);
      let result: unknown;
      if (request.method === "GET" && path === "/console/manifest") {
        result = await app.executor.call(app.manifest, context, undefined);
      } else if (request.method === "POST" && path.startsWith("/console/actions/")) {
        result = await app.consoleExtension.call(context, decodeURIComponent(path.slice(17)), await jsonBody(request));
      } else if (request.method === "POST" && path === "/worker/drain") {
        result = await app.executor.callExternal(app.modules.notifications.actions.drain, context, await jsonBody(request));
      } else {
        throw new ActionError("not_found", "Route not found");
      }
      response.writeHead(200, { "content-type": "application/json" });
      response.end(JSON.stringify(result));
    } catch (error) {
      const failure = error instanceof ActionError ? error : new ActionError("internal", "Request failed", { cause: error });
      if (failure.code === "internal") console.error(failure);
      response.writeHead(status[failure.code], { "content-type": "application/json" });
      response.end(JSON.stringify({ error: { code: failure.code, message: failure.message } }));
    }
  });
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const filename = process.env.LENSO_DB ?? ".local/team-desk.sqlite";
  mkdirSync(dirname(filename), { recursive: true });
  const store = new Store(filename, identitySchema.parse(JSON.parse(process.env.LENSO_IDENTITIES ?? "null")));
  const server = serve(store);
  server.listen(Number(process.env.PORT ?? 3000), "127.0.0.1", () => {
    console.log("Team Desk:", server.address());
  });
  for (const signal of ["SIGINT", "SIGTERM"] as const) {
    process.once(signal, () => { server.close(() => store.close()); });
  }
}
