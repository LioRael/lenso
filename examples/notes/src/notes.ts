import { definePlugin, type Plugin } from "lenso/plugin";

export interface NoteInput {
  title: string;
  body?: string;
}

export interface StoredNote {
  id: string;
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
  list(): Promise<StoredNote[]>;
  update(id: string, input: { title: string; body: string }): Promise<StoredNote | null>;
  remove(id: string): Promise<boolean>;
}

export interface NotesService {
  create(input: NoteInput): Promise<Note>;
  list(): Promise<Note[]>;
  update(id: string, input: NoteInput): Promise<Note | null>;
  remove(id: string): Promise<boolean>;
}

export class NoteInputError extends Error {}

function validate(input: NoteInput): { title: string; body: string } {
  const title = input.title.trim();
  const body = input.body ?? "";
  if (!title || title.length > 200) {
    throw new NoteInputError("Title must contain 1 to 200 characters");
  }
  if (body.length > 20_000) throw new NoteInputError("Body must contain at most 20000 characters");
  return { title, body };
}

function present(note: StoredNote): Note {
  return { ...note, createdAt: note.createdAt.toISOString() };
}

export function createNotesService(queries: NotesQueries): NotesService {
  return {
    async create(input) {
      const values = validate(input);
      return present(
        await queries.insert({ ...values, id: crypto.randomUUID(), createdAt: new Date() }),
      );
    },
    async list() {
      return (await queries.list()).map(present);
    },
    async update(id, input) {
      const note = await queries.update(id, validate(input));
      return note ? present(note) : null;
    },
    async remove(id) {
      return queries.remove(id);
    },
  };
}

/** Exact plugin references bind each service instance to its chosen database. */
export function createNotesPlugin<TDatabase>(options: {
  id: string;
  database: Plugin<TDatabase>;
  queries(database: TDatabase): NotesQueries;
}): Plugin<NotesService> {
  return definePlugin({
    id: options.id,
    requires: [options.database],
    setup(context) {
      return createNotesService(options.queries(context.get(options.database)));
    },
  });
}
