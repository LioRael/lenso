import { createMemoryProvider } from "@lenso/realtime/memory";
import { createNotesRealtime, type Note, type NotesPort } from "./notes";
import type { Identity } from "@lenso/realtime";

// Trusted in-process fixture, not a login endpoint or production authentication.
const principal = Object.freeze({});
const identity: Identity<object> = {
  scope: "single-application",
  subject: "note-owner",
  principal,
  expiresAt: Date.now() + 60000,
};
let stored: Note = { id: "welcome", title: "Notes", body: "Initial text", revision: 1 };
const notes: NotesPort<object> = {
  async read(actor, id, signal) {
    signal.throwIfAborted();
    if (actor.principal !== principal || actor.scope !== identity.scope) throw new Error("Denied");
    return id === stored.id ? { ...stored } : null;
  },
  async update(actor, id, input) {
    if (actor.principal !== principal || actor.scope !== identity.scope) throw new Error("Denied");
    if (id !== stored.id) return null;
    stored = { ...stored, ...input, revision: stored.revision + 1 };
    return { ...stored };
  },
};
const bridge = await createNotesRealtime(notes, createMemoryProvider());
try {
  const first = await bridge.open(identity, "welcome");
  const second = await bridge.open(identity, "welcome");
  const a = first.connection[Symbol.asyncIterator](),
    b = second.connection[Symbol.asyncIterator]();
  await a.next();
  await b.next();
  console.log(JSON.stringify({ snapshot: first.snapshot }));
  await bridge.update(identity, "welcome", { title: "Notes", body: "Updated text" });
  console.log(JSON.stringify({ first: (await a.next()).value, second: (await b.next()).value }));
  first.subscription.unsubscribe();
  first.connection.close();
  second.connection.close();
} finally {
  await bridge.close();
}
