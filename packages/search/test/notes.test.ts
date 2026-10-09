import { expect, test } from "bun:test";
import { createAuth, defineSource, realm } from "@lenso/auth";
import type { NotesAuthentication } from "../../../examples/notes/src/auth";
import {
  createNotesService,
  notesAudiences,
  type NotesActor,
  type NotesOperation,
  type StoredNote,
} from "../../../examples/notes/src/notes";
import { createNotesSearchAdapter, NotesProjectionPending } from "../examples/notes";
import type { NotesRepairTarget } from "../examples/notes";
import type { SearchDocument, SearchQuery, SearchScope, SearchService } from "../src/index";

function fixture(withTenant = true, withRepairTasks = true) {
  let active = true;
  let tenant = "tenant-1";
  let failProjection = false;
  let failRecording = false;
  let authorizations = 0;
  let reads = 0;
  const rows = new Map<string, StoredNote>();
  const documents = new Map<string, SearchDocument>();
  const tickets = new Map<string, NotesRepairTarget>();
  const queries: { scope: SearchScope; input: SearchQuery }[] = [];
  const source = defineSource<string | null, string>({
    async verify(evidence) {
      return active && (evidence === "A" || evidence === "B")
        ? { status: "verified", subjectId: evidence, kind: "user" }
        : { status: "rejected" };
    },
  });
  const auth = createAuth(realm("notes", source));
  const unused = async (): Promise<never> => {
    throw new Error("Session management is not used by this fixture");
  };
  const authentication: NotesAuthentication = {
    ...auth,
    issue: unused,
    renew: unused,
    revoke: unused,
  };
  const notes = createNotesService(
    {
      async insert(note) {
        rows.set(note.id, note);
        return note;
      },
      async list() {
        throw new Error("Search must not list or scan Notes");
      },
      async read(id) {
        return rows.get(id) ?? null;
      },
      async update(id, ownerId, input) {
        const note = rows.get(id);
        if (!note || note.ownerId !== ownerId) return null;
        const updated = { ...note, ...input };
        rows.set(id, updated);
        return updated;
      },
      async remove(id, ownerId) {
        return rows.get(id)?.ownerId === ownerId && rows.delete(id);
      },
    },
    authentication,
  );
  const search: SearchService = {
    capabilities: {
      fullText: true,
      sorts: ["relevance", "id"],
      pagination: "offset",
      summary: "plain-text",
      count: "exact",
    },
    async upsert(scope, document) {
      if (failProjection) throw new Error("Projection unavailable");
      documents.set(`${scope.ownerId}:${document.id}`, document);
    },
    async delete(scope, reference) {
      if (failProjection) throw new Error("Projection unavailable");
      documents.delete(`${scope.ownerId}:${reference.id}`);
    },
    async query(scope, input) {
      queries.push({ scope, input });
      return { hits: [] };
    },
  };
  const adapter = createNotesSearchAdapter({
    namespace: "private-notes",
    notes,
    authentication,
    audiences: notesAudiences,
    search,
    ...(withTenant ? { tenantForOwner: async () => tenant } : {}),
    async readLatest(target) {
      reads++;
      expect(target.scope.namespace).toBe("private-notes");
      expect(target.scope.tenantId).toBe(withTenant ? tenant : undefined);
      const note = rows.get(target.id);
      return note?.ownerId === target.scope.ownerId
        ? { ...note, createdAt: note.createdAt.toISOString() }
        : null;
    },
    ...(withRepairTasks
      ? {
          repairTasks: {
            async recordRepair(target) {
              if (failRecording) throw new Error("Ticket store unavailable");
              const ticket = crypto.randomUUID();
              tickets.set(ticket, structuredClone(target));
              return ticket;
            },
            async authorizeRepair(ticket) {
              authorizations++;
              const target = tickets.get(ticket);
              if (!active || !target) throw new Error("Repair not authorized");
              return { ...target, scope: { ...target.scope, tenantId: tenant } };
            },
          },
        }
      : {}),
  });
  return {
    adapter,
    rows,
    documents,
    tickets,
    queries,
    actor: async <O extends NotesOperation>(operation: O, subject = "A"): Promise<NotesActor<O>> =>
      (await authentication.for(notesAudiences[operation]).required(subject)) as NotesActor<O>,
    setActive: (value: boolean) => {
      active = value;
    },
    setTenant: (value: string) => {
      tenant = value;
    },
    failProjection: (value: boolean) => {
      failProjection = value;
    },
    failRecording: (value: boolean) => {
      failRecording = value;
    },
    counts: () => ({ authorizations, reads }),
    close: () => auth.close(),
  };
}

test("exact Notes CRUD commits first and projects scoped latest state without listing", async () => {
  const f = fixture();
  try {
    const note = await f.adapter.notes.create(await f.actor("create"), {
      title: "First",
      body: "Original",
    });
    expect(f.rows.get(note.id)?.title).toBe("First");
    expect(f.documents.get(`A:${note.id}`)).toEqual({
      id: note.id,
      type: "note",
      ownerId: "A",
      tenantId: "tenant-1",
      title: "First",
      body: "Original",
    });
    expect(await f.adapter.notes.read(await f.actor("read"), note.id)).toEqual(note);
    await f.adapter.notes.update(await f.actor("update"), note.id, {
      title: "Updated",
      body: "Latest",
    });
    expect(f.documents.get(`A:${note.id}`)?.body).toBe("Latest");
    await expect(
      f.adapter.notes.update(await f.actor("update", "B"), note.id, {
        title: "Forbidden",
      }),
    ).rejects.toMatchObject({ code: "FORBIDDEN" });
    expect(f.documents.get(`A:${note.id}`)?.title).toBe("Updated");
    expect(await f.adapter.notes.remove(await f.actor("remove"), note.id)).toBe(true);
    expect(f.rows.has(note.id)).toBe(false);
    expect(f.documents.size).toBe(0);
    expect(
      await f.adapter.notes.update(await f.actor("update"), note.id, {
        title: "Missing",
      }),
    ).toBeNull();
    expect(await f.adapter.notes.remove(await f.actor("remove"), note.id)).toBe(false);
    expect(f.counts().reads).toBe(3);
  } finally {
    await f.close();
  }
});

test("query uses the existing list audience and trusted current-user tenant scope only", async () => {
  const f = fixture();
  try {
    const input = {
      text: "hello",
      pageSize: 7,
      sort: "id" as const,
      namespace: "other",
      ownerId: "B",
      tenantId: "other",
      type: "secret",
    };
    await expect(f.adapter.query(null, input)).rejects.toMatchObject({ code: "UNAUTHORIZED" });
    expect(await f.adapter.query(await f.actor("list"), input)).toEqual({ hits: [] });
    expect(f.queries).toEqual([
      {
        scope: { namespace: "private-notes", ownerId: "A", tenantId: "tenant-1" },
        input: {
          text: "hello",
          pageSize: 7,
          sort: "id",
          type: "note",
          cursor: undefined,
          includeTotal: undefined,
        },
      },
    ]);
    expect(f.counts().reads).toBe(0);
    const wrongAudience = await f.actor("read");
    await expect(
      f.adapter.query(wrongAudience as unknown as NotesActor<"list">, input),
    ).rejects.toMatchObject({ code: "UNAUTHORIZED" });
    const forged = {
      realmId: "notes",
      subjectId: "B",
      audience: "notes:list",
      kind: "user",
    } as NotesActor<"list">;
    await expect(f.adapter.query(forged, input)).rejects.toMatchObject({ code: "UNAUTHORIZED" });
    const actor = await f.actor("list");
    f.setActive(false);
    await expect(f.adapter.query(actor, input)).rejects.toMatchObject({ code: "UNAUTHORIZED" });
    expect(f.queries.length).toBe(1);
  } finally {
    await f.close();
  }
});

test("owner-only Notes works without a tenant resolver", async () => {
  const f = fixture(false);
  try {
    const note = await f.adapter.notes.create(await f.actor("create"), { title: "Owner only" });
    expect(f.documents.get(`A:${note.id}`)).toEqual({
      id: note.id,
      type: "note",
      ownerId: "A",
      title: "Owner only",
      body: "",
    });
    expect(await f.adapter.query(await f.actor("list"), { text: "Owner" })).toEqual({ hits: [] });
    expect(f.queries[0]?.scope).toEqual({ namespace: "private-notes", ownerId: "A" });
  } finally {
    await f.close();
  }
});

test("direct repair derives fresh read scope without Tasks tickets", async () => {
  const f = fixture(true, false);
  try {
    const note = await f.adapter.notes.create(await f.actor("create"), { title: "repair me" });
    f.failProjection(true);
    await expect(
      f.adapter.notes.update(await f.actor("update"), note.id, { title: "latest" }),
    ).rejects.toBeInstanceOf(NotesProjectionPending);
    f.failProjection(false);
    await f.adapter.repair(await f.actor("read"), note.id);
    expect(f.documents.get(`A:${note.id}`)?.title).toBe("latest");
    const reads = f.counts().reads;
    for (const actor of [
      null,
      { subjectId: "A" } as unknown as NotesActor<"read">,
      (await f.actor("list")) as unknown as NotesActor<"read">,
    ]) {
      await expect(f.adapter.repair(actor, note.id)).rejects.toMatchObject({
        code: "UNAUTHORIZED",
      });
    }
    expect(f.counts().reads).toBe(reads);
    await expect(f.adapter.repair(await f.actor("read", "B"), note.id)).resolves.toBeUndefined();
    expect(f.counts().reads).toBe(reads + 1);
    const revoked = await f.actor("read");
    f.setActive(false);
    await expect(f.adapter.repair(revoked, note.id)).rejects.toMatchObject({
      code: "UNAUTHORIZED",
    });
    expect(() => f.adapter.createRepairTask()).toThrow();
    await expect(f.adapter.repair(revoked, "not-an-id")).rejects.toMatchObject({
      code: "invalid-input",
    });
  } finally {
    await f.close();
  }
});

test("bounded repair tasks reauthorize each attempt and reread current scoped content", async () => {
  const f = fixture();
  try {
    f.failProjection(true);
    const failure = await f.adapter.notes
      .create(await f.actor("create"), {
        title: "Committed",
        body: "Old snapshot",
      })
      .catch((error: unknown) => error);
    expect(failure).toBeInstanceOf(NotesProjectionPending);
    if (!(failure instanceof NotesProjectionPending)) throw new Error("Expected pending error");
    expect(failure.businessCommitted).toBe(true);
    expect(failure.projectionPending).toBe(true);
    expect(JSON.stringify(failure)).not.toContain("Old snapshot");
    expect(failure.cause).toBeUndefined();
    const ticket = failure.repairTicket!;
    expect(Object.keys(f.tickets.get(ticket)!)).toEqual(["scope", "id"]);
    const target = f.tickets.get(ticket)!;
    const task = f.adapter.createRepairTask();
    expect(task.maxAttempts).toBe(3);
    expect(task.retry).toEqual({ delaySeconds: 5, backoff: true, maxDelaySeconds: 60 });
    const context = {
      jobId: crypto.randomUUID(),
      attempt: 1,
      signal: new AbortController().signal,
    };
    await expect(task.handler({ ticket }, context)).rejects.toThrow("Notes search repair failed");
    f.setTenant("tenant-2");
    const row = f.rows.get(target.id)!;
    f.rows.set(row.id, { ...row, body: "Current content" });
    f.failProjection(false);
    await task.handler({ ticket }, { ...context, attempt: 2 });
    expect(f.documents.get(`A:${row.id}`)).toMatchObject({
      body: "Current content",
      tenantId: "tenant-2",
    });
    await task.handler({ ticket }, { ...context, attempt: 3 });
    expect(f.documents.size).toBe(1);
    expect(f.counts().authorizations).toBe(3);
    f.setActive(false);
    const reads = f.counts().reads;
    await expect(f.adapter.repairTicket(ticket)).rejects.toMatchObject({ code: "repair-failed" });
    expect(f.counts().reads).toBe(reads);
    f.setActive(true);
    f.rows.delete(row.id);
    await f.adapter.repairTicket(ticket);
    expect(f.documents.size).toBe(0);
    await expect(f.adapter.repairTicket("unknown")).rejects.toMatchObject({
      code: "repair-failed",
    });
    const parsed = await task.input["~standard"].validate({
      ticket,
      ownerId: "B",
      body: "stale",
    });
    expect(parsed.issues).toBeDefined();
  } finally {
    await f.close();
  }
});

test("ticket recording failure still reports the business commit; business rejection does not repair", async () => {
  const f = fixture();
  try {
    f.failProjection(true);
    f.failRecording(true);
    const failure = await f.adapter.notes
      .create(await f.actor("create"), {
        title: "Committed",
      })
      .catch((error: unknown) => error);
    expect(failure).toBeInstanceOf(NotesProjectionPending);
    if (!(failure instanceof NotesProjectionPending)) throw new Error("Expected pending error");
    expect(failure.repairTicket).toBeUndefined();
    expect(failure.repairRecordingFailed).toBe(true);
    expect(f.rows.size).toBe(1);
    const reads = f.counts().reads;
    await expect(
      f.adapter.notes.create(await f.actor("create"), {
        title: " ",
      }),
    ).rejects.toThrow("Invalid note input");
    expect(f.rows.size).toBe(1);
    expect(f.counts().reads).toBe(reads);
  } finally {
    await f.close();
  }
});
