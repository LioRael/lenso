import {
  createRealtime,
  RealtimeError,
  type Identity,
  type RealtimeProvider,
} from "@lenso/realtime";

export interface Note {
  id: string;
  title: string;
  body: string;
  revision: number;
}

/** Implement with the existing authorized Notes service, not a second HTTP policy. */
export interface NotesPort<P> {
  read(identity: Identity<P>, id: string, signal: AbortSignal): Promise<Note | null>;
  update(
    identity: Identity<P>,
    id: string,
    input: { title: string; body: string },
  ): Promise<Note | null>;
}

export async function createNotesRealtime<P>(notes: NotesPort<P>, provider: RealtimeProvider) {
  const realtime = await createRealtime<P>({
    provider,
    async authorize(identity, resource, signal) {
      if (resource.type !== "note") return false;
      const note = await notes.read(identity, resource.id, signal);
      return note ? { validUntil: identity.expiresAt } : false;
    },
  });
  return {
    realtime,
    async open(identity: Identity<P>, id: string, signal?: AbortSignal, cursor?: string) {
      const connection = realtime.connect(identity, { signal });
      try {
        const subscription = await connection.subscribe(
          { scope: identity.scope, type: "note", id },
          { cursor },
        );
        const snapshot = await connection.snapshot(subscription, (snapshotSignal) =>
          notes.read(identity, id, snapshotSignal),
        );
        if (!snapshot.value) throw new RealtimeError("denied");
        return { connection, subscription, snapshot };
      } catch (error) {
        connection.close();
        throw error;
      }
    },
    async update(identity: Identity<P>, id: string, input: { title: string; body: string }) {
      const note = await notes.update(identity, id, input);
      if (note) {
        // Publish invalidation after the durable write, never uncommitted note contents.
        await realtime.publish({ scope: identity.scope, type: "note", id }, "note.updated", {
          revision: note.revision,
        });
      }
      return note;
    },
    close: () => realtime.close(),
  };
}
