import { z } from "zod";

export const noteInput = z.strictObject({
  title: z.string().trim().min(1).max(200),
  body: z.string().max(20_000).optional(),
});
export const noteId = z.string().uuid();
export const noteLookupInput = z.strictObject({ id: noteId });
export const noteUpdateInput = noteInput.extend({ id: noteId });
export const noteOutput = z.strictObject({
  id: noteId,
  ownerId: z.string(),
  title: z.string(),
  body: z.string(),
  createdAt: z.string(),
});
export const notesListInput = z.strictObject({});
export const noteFileInput = z.strictObject({ fileId: z.string().uuid() });
export type NoteInput = z.input<typeof noteInput>;
