import { expect, test } from "bun:test";
import { AuthError } from "@lenso/auth";
import { createNotesSearchAdapter, NotesProjectionPending } from "../examples/notes";
import type { NotesSearchHost } from "../examples/notes";
import type { NotesActor } from "../../../examples/notes/src/notes";

function fixture() {
  let fault = "";
  const owner = "owner";
  const id = crypto.randomUUID();
  const secret = "PRIVATE_CALLBACK_SQL_CONNECTION_BODY";
  function check(point: string) {
    if (fault === point)
      throw Object.assign(new Error(secret), {
        query: secret,
        body: secret,
        connection: `postgres://${secret}`,
      });
  }
  let present = true;
  let deletes = 0;
  const host = {
    namespace: "notes",
    audiences: { read: {}, list: {}, create: {}, update: {}, remove: {} },
    authentication: {
      for() {
        return {
          async enforce(actor: unknown) {
            if (!actor) throw new AuthError("UNAUTHORIZED", { cause: new Error(secret) });
            return { subjectId: owner, kind: "user" };
          },
        };
      },
    },
    tenantForOwner: async () => {
      check("membership");
      return "tenant";
    },
    notes: {
      async create() {
        return { id, ownerId: owner, title: "title", body: secret, createdAt: "" };
      },
      async remove() {
        present = false;
        return true;
      },
    },
    search: {
      async upsert() {
        check("projection");
      },
      async delete() {
        check("projection");
        deletes++;
      },
      async query() {
        check("query");
        return { hits: [] };
      },
    },
    async readLatest() {
      check("read");
      return present ? { id, ownerId: owner, title: "title", body: secret, createdAt: "" } : null;
    },
    repairTasks: {
      async recordRepair() {
        check("recording");
        return "ticket";
      },
      async authorizeRepair() {
        check("authority");
        return {
          id,
          scope: { namespace: "notes", ownerId: owner, tenantId: "tenant" },
        };
      },
    },
  } as unknown as NotesSearchHost;
  return {
    host,
    id,
    secret,
    fault: (value: string) => {
      fault = value;
    },
    deletes: () => deletes,
  };
}

test("adapter host callback failures are structured and never retain private driver data", async () => {
  const f = fixture();
  const adapter = createNotesSearchAdapter(f.host);
  const actor = {} as NotesActor<"read">;
  async function safeFailure(run: () => Promise<unknown>, code: string) {
    const error = await run().catch((failure: unknown) => failure);
    expect(error).toMatchObject({ code });
    expect(JSON.stringify(error)).not.toContain(f.secret);
    expect((error as Error).message).not.toContain(f.secret);
    expect((error as Error).cause).toBeUndefined();
  }
  f.fault("read");
  await safeFailure(() => adapter.repair(actor, f.id), "repair-failed");
  await safeFailure(() => adapter.repairTicket("ticket"), "repair-failed");
  f.fault("authority");
  const task = adapter.createRepairTask();
  await safeFailure(
    () =>
      task.handler(
        { ticket: "ticket" },
        {
          jobId: crypto.randomUUID(),
          attempt: 1,
          signal: new AbortController().signal,
        },
      ),
    "repair-failed",
  );
  for (const point of ["membership", "query"]) {
    f.fault(point);
    await safeFailure(
      () => adapter.query({} as NotesActor<"list">, { text: "term" }),
      "query-failed",
    );
  }
  await safeFailure(() => adapter.repair(null, f.id), "UNAUTHORIZED");
  f.fault("projection");
  await safeFailure(
    () => adapter.notes.create({} as NotesActor<"create">, { title: "title" }),
    "projection-pending",
  );
  f.fault("recording");
  f.host.readLatest = async () => {
    throw new Error(f.secret);
  };
  const error = await adapter.notes
    .create({} as NotesActor<"create">, { title: "title" })
    .catch((failure: unknown) => failure);
  expect(error).toMatchObject({ code: "projection-pending", repairRecordingFailed: true });
  expect(JSON.stringify(error)).not.toContain(f.secret);
});

test("pending deletion retains ID for explicit repair without a ticket store", async () => {
  const f = fixture();
  delete f.host.repairTasks;
  const adapter = createNotesSearchAdapter(f.host);
  f.fault("projection");
  const pending = await adapter.notes
    .remove({} as NotesActor<"remove">, f.id)
    .catch((error: unknown) => error);
  expect(pending).toBeInstanceOf(NotesProjectionPending);
  expect(pending).toMatchObject({
    documentId: f.id,
    removed: true,
    businessCommitted: true,
    repairRecordingFailed: false,
  });
  expect(JSON.stringify(pending)).not.toContain(f.secret);
  f.fault("");
  await adapter.repair({} as NotesActor<"read">, f.id);
  expect(f.deletes()).toBe(1);
});
