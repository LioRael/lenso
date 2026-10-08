import { ORPCError, os } from "@orpc/server";
import { z } from "zod";
import { NoteInputError, type NotesService } from "./notes";

export function createNotesRouter(service: NotesService) {
  const input = z.object({ title: z.string(), body: z.string().optional() });
  const id = z.string().uuid();
  const procedure = os.use(async ({ next }) => {
    try {
      return await next();
    } catch (error) {
      if (error instanceof NoteInputError)
        throw new ORPCError("BAD_REQUEST", { message: error.message });
      throw error;
    }
  });
  return {
    create: procedure.input(input).handler(({ input }) => service.create(input)),
    list: procedure.handler(() => service.list()),
    update: procedure
      .input(input.extend({ id }))
      .handler(({ input: { id, ...input } }) => service.update(id, input)),
    remove: procedure
      .input(z.object({ id }))
      .handler(async ({ input }) => ({ removed: await service.remove(input.id) })),
  };
}

export type NotesRouter = ReturnType<typeof createNotesRouter>;
