import { ORPCError, os } from "@orpc/server";
import { bearerEvidence, type FetchAuthContext } from "@lenso/auth/fetch";
import { requiredAuth } from "@lenso/auth/orpc";
import type { NotesAuthentication } from "./auth";
import { NoteInputError, notesAudiences, type NotesService } from "./notes";
import { noteInput, noteLookupInput, noteUpdateInput, noteOutput } from "./contracts";

export { noteInput, noteId } from "./contracts";

export function createNotesRouter(service: NotesService, authentication: NotesAuthentication) {
  const procedure = os.$context<FetchAuthContext>().use(async ({ next }) => {
    try {
      return await next();
    } catch (error) {
      if (error instanceof NoteInputError)
        throw new ORPCError("BAD_REQUEST", { message: "Invalid note input", cause: error });
      throw error;
    }
  });
  return {
    create: procedure
      .use(requiredAuth(authentication.for(notesAudiences.create), bearerEvidence))
      .input(noteInput)
      .handler(({ input, context }) => service.create(context.actor, input)),
    list: procedure
      .use(requiredAuth(authentication.for(notesAudiences.list), bearerEvidence))
      .output(noteOutput.array())
      .handler(({ context }) => service.list(context.actor)),
    read: procedure
      .use(requiredAuth(authentication.for(notesAudiences.read), bearerEvidence))
      .input(noteLookupInput)
      .output(noteOutput.nullable())
      .handler(({ input, context }) => service.read(context.actor, input.id)),
    update: procedure
      .use(requiredAuth(authentication.for(notesAudiences.update), bearerEvidence))
      .input(noteUpdateInput)
      .handler(({ input: { id, ...input }, context }) => service.update(context.actor, id, input)),
    remove: procedure
      .use(requiredAuth(authentication.for(notesAudiences.remove), bearerEvidence))
      .input(noteLookupInput)
      .handler(async ({ input, context }) => ({
        removed: await service.remove(context.actor, input.id),
      })),
  };
}

export type NotesRouter = ReturnType<typeof createNotesRouter>;
