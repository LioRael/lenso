import { AuthError } from "@lenso/auth";
import { authErrorResponse, bearerEvidence } from "@lenso/auth/fetch";
import { createWebPlugin } from "@lenso/web";
import type { Plugin } from "@lenso/core";
import { z } from "zod";
import type { NotesAuthentication } from "./auth";
import { NoteInputError, notesAudiences, type NotesService } from "./notes";
import { createNotesRouter } from "./router";
import { noteId, noteInput } from "./contracts";

const loginInput = z.strictObject({ key: z.string().regex(/^[0-9a-fA-F]{64}$/) });
async function jsonInput<S extends z.ZodType>(request: Request, schema: S): Promise<z.output<S>> {
  try {
    return schema.parse(await request.json());
  } catch {
    request.signal.throwIfAborted();
    throw new NoteInputError("Invalid request input");
  }
}

export function createNotesWebPlugin(
  notes: Plugin<NotesService>,
  authentication: Plugin<NotesAuthentication>,
) {
  return createWebPlugin({
    id: "notes-web",
    requires: [notes, authentication],
    router: (context) => createNotesRouter(context.get(notes), context.get(authentication)),
    fetch: (context) => {
      const service = context.get(notes);
      const auth = context.get(authentication);
      return async (webContext) => {
        const { request, signal } = webContext;
        const path = new URL(request.url).pathname;
        const item = /^\/notes\/([^/]+)$/.exec(path);
        if (
          path !== "/notes" &&
          !item &&
          !["/session", "/session/renew", "/session/revoke"].includes(path)
        )
          return undefined;
        try {
          signal.throwIfAborted();
          const allowed = path.startsWith("/session")
            ? ["POST"]
            : item
              ? ["GET", "PATCH", "DELETE"]
              : ["POST", "GET"];
          if (!allowed.includes(request.method))
            return Response.json(
              { code: "METHOD_NOT_ALLOWED", message: "Method not allowed" },
              { status: 405, headers: { allow: allowed.join(", ") } },
            );
          if (path === "/session") {
            const { key } = await jsonInput(request, loginInput);
            return Response.json(await auth.issue(key, { signal }));
          }
          const evidence = bearerEvidence(webContext);
          if (path === "/session/renew" || path === "/session/revoke") {
            if (!evidence.evidence) throw new AuthError("UNAUTHORIZED");
            if (path === "/session/renew")
              return Response.json(await auth.renew(evidence.evidence, evidence));
            await auth.revoke(evidence.evidence, evidence);
            return Response.json({ revoked: true });
          }
          if (!item && request.method === "GET")
            return Response.json(
              await service.list(
                await auth.for(notesAudiences.list).required(evidence.evidence, evidence),
              ),
            );
          if (!item) {
            const actor = await auth
              .for(notesAudiences.create)
              .required(evidence.evidence, evidence);
            return Response.json(await service.create(actor, await jsonInput(request, noteInput)), {
              status: 201,
            });
          }
          const id = noteId.safeParse(item[1]);
          if (!id.success) throw new NoteInputError("Invalid note id");
          if (request.method === "GET")
            return Response.json(
              await service.read(
                await auth.for(notesAudiences.read).required(evidence.evidence, evidence),
                id.data,
              ),
            );
          if (request.method === "DELETE")
            return Response.json({
              removed: await service.remove(
                await auth.for(notesAudiences.remove).required(evidence.evidence, evidence),
                id.data,
              ),
            });
          const actor = await auth.for(notesAudiences.update).required(evidence.evidence, evidence);
          return Response.json(
            await service.update(actor, id.data, await jsonInput(request, noteInput)),
          );
        } catch (error) {
          signal.throwIfAborted();
          if (error instanceof AuthError) return authErrorResponse(error);
          if (error instanceof NoteInputError)
            return Response.json({ code: "BAD_REQUEST", message: error.message }, { status: 400 });
          return Response.json(
            { code: "SERVICE_UNAVAILABLE", message: "Notes unavailable" },
            { status: 503 },
          );
        }
      };
    },
  });
}
