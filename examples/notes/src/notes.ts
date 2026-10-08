import { definePlugin, type Plugin } from "lenso/plugin";
import { audience, type Actor } from "@lenso/auth";
import type { NotesAuthentication } from "./auth";
import { noteInput, noteLookupInput, type NoteInput } from "./contracts";

export type { NoteInput } from "./contracts";

export const notesAudiences = {
  create: audience("notes:create"),
  list: audience("notes:list"),
  read: audience("notes:read"),
  update: audience("notes:update"),
  remove: audience("notes:remove"),
  fileMetadata: audience("notes:file-metadata"),
  fileDelete: audience("notes:file-delete"),
} as const;
export type NotesOperation = keyof typeof notesAudiences;
export type NotesActor<O extends NotesOperation> = Actor<
  "notes",
  string,
  (typeof notesAudiences)[O]["id"]
>;

export interface StoredNote {
  id: string;
  readonly ownerId: string;
  title: string;
  body: string;
  createdAt: Date;
}

export interface Note extends Omit<StoredNote, "createdAt"> {
  createdAt: string;
}

/** Only the queries this business needs; each dialect keeps its real Drizzle type. */
export interface NotesQueries {
  insert(note: StoredNote): Promise<StoredNote>;
  list(ownerId: string): Promise<StoredNote[]>;
  read(id: string): Promise<StoredNote | null>;
  update(
    id: string,
    ownerId: string,
    input: { title: string; body: string },
  ): Promise<StoredNote | null>;
  remove(id: string, ownerId: string): Promise<boolean>;
}

export interface NotesService {
  create(actor: NotesActor<"create"> | null, input: NoteInput): Promise<Note>;
  list(actor: NotesActor<"list"> | null): Promise<Note[]>;
  read(actor: NotesActor<"read"> | null, id: string): Promise<Note | null>;
  update(actor: NotesActor<"update"> | null, id: string, input: NoteInput): Promise<Note | null>;
  remove(actor: NotesActor<"remove"> | null, id: string): Promise<boolean>;
}

export class NoteInputError extends Error {}

function validate(input: NoteInput): { title: string; body: string } {
  const parsed = noteInput.safeParse(input);
  if (!parsed.success) throw new NoteInputError("Invalid note input");
  return { title: parsed.data.title, body: parsed.data.body ?? "" };
}

function present(note: StoredNote): Note {
  return { ...note, createdAt: note.createdAt.toISOString() };
}

export function createNotesService(
  queries: NotesQueries,
  authentication: NotesAuthentication,
): NotesService {
  async function authorize<O extends NotesOperation>(
    operation: O,
    actor: NotesActor<O> | null,
    note?: StoredNote,
  ) {
    return authentication
      .for(notesAudiences[operation])
      .enforce(
        actor,
        note,
        ({ principal, resource }) =>
          principal.kind === "user" &&
          (resource === undefined || resource.ownerId === principal.subjectId),
      );
  }
  async function owned<O extends "read" | "update" | "remove">(
    operation: O,
    actor: NotesActor<O> | null,
    id: string,
  ) {
    await authorize(operation, actor);
    if (!noteLookupInput.safeParse({ id }).success) throw new NoteInputError("Invalid note id");
    const note = await queries.read(id);
    if (note) await authorize(operation, actor, note);
    return note;
  }
  return {
    async create(actor, input) {
      const principal = await authorize("create", actor);
      const values = validate(input);
      return present(
        await queries.insert({
          ...values,
          ownerId: principal.subjectId,
          id: crypto.randomUUID(),
          createdAt: new Date(),
        }),
      );
    },
    async list(actor) {
      const principal = await authorize("list", actor);
      const rows = await queries.list(principal.subjectId);
      for (const row of rows) await authorize("list", actor, row);
      return rows.map(present);
    },
    async read(actor, id) {
      const note = await owned("read", actor, id);
      return note ? present(note) : null;
    },
    async update(actor, id, input) {
      const existing = await owned("update", actor, id);
      const values = validate(input);
      if (!existing) return null;
      const note = await queries.update(id, existing.ownerId, values);
      return note ? present(note) : null;
    },
    async remove(actor, id) {
      const existing = await owned("remove", actor, id);
      return existing ? queries.remove(id, existing.ownerId) : false;
    },
  };
}

/** Exact plugin references bind each service instance to its chosen database. */
export function createNotesPlugin<TDatabase>(options: {
  id: string;
  database: Plugin<TDatabase>;
  authentication: Plugin<NotesAuthentication>;
  queries(database: TDatabase): NotesQueries;
}): Plugin<NotesService> {
  return definePlugin({
    id: options.id,
    requires: [options.database, options.authentication],
    setup(context) {
      return createNotesService(
        options.queries(context.get(options.database)),
        context.get(options.authentication),
      );
    },
  });
}
