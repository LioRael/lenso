import { defineTask } from "@lenso/tasks";
import { AuthError } from "@lenso/auth";
import { SearchError, type SearchQuery, type SearchScope, type SearchService } from "@lenso/search";
import { z } from "zod";
import type { NotesAuthentication } from "../../../examples/notes/src/auth";
import type {
  NotesActor,
  NotesOperation,
  NotesService,
  Note,
  notesAudiences,
} from "../../../examples/notes/src/notes";
import { noteLookupInput } from "../../../examples/notes/src/contracts";

export class NotesSearchError extends Error {
  constructor(readonly code: "repair-failed" | "query-failed") {
    super(code === "repair-failed" ? "Notes search repair failed" : "Notes search query failed");
    this.name = "NotesSearchError";
  }
}

export interface NotesRepairTarget {
  readonly scope: SearchScope & { ownerId: string };
  readonly id: string;
}

export interface NotesSearchHost {
  namespace: string;
  notes: NotesService;
  authentication: NotesAuthentication;
  audiences: typeof notesAudiences;
  search: SearchService;
  /** Trusted membership lookup, never a tenant supplied in business JSON. */
  tenantForOwner?(ownerId: string): Promise<string | undefined>;
  /** Query by scope AND id in the database; null means absent in that scope. */
  readLatest(target: NotesRepairTarget): Promise<Note | null>;
  repairTasks?: {
    /** Persist only the authorized target, not note text; return an opaque ticket. */
    recordRepair(target: NotesRepairTarget): Promise<string>;
    /** Resolve stored authority and recheck current owner/tenant access each attempt. */
    authorizeRepair(ticket: string): Promise<NotesRepairTarget>;
  };
}

export class NotesProjectionPending extends Error {
  readonly code = "projection-pending";
  readonly businessCommitted = true;
  readonly projectionPending = true;
  readonly documentId?: string;
  readonly removed?: boolean;

  constructor(
    result: Note | boolean,
    readonly repairTicket: string | undefined,
    readonly repairRecordingFailed = false,
    documentId?: string,
  ) {
    super(
      "Notes operation committed; search projection pending. Do not replay the business write.",
    );
    this.name = "NotesProjectionPending";
    if (typeof result === "boolean") {
      this.removed = result;
      this.documentId = documentId;
    } else this.documentId = result.id;
  }
}

const repairInput = z.strictObject({ ticket: z.string().min(1).max(256) });

export function createNotesSearchAdapter(host: NotesSearchHost) {
  function safeError(error: unknown, code: "repair-failed" | "query-failed"): Error {
    if (error instanceof AuthError) return new AuthError(error.code);
    if (error instanceof SearchError) return new SearchError(error.code);
    return new NotesSearchError(code);
  }

  async function scopeFor<O extends NotesOperation>(
    operation: O,
    actor: NotesActor<O> | null,
  ): Promise<NotesRepairTarget["scope"]> {
    try {
      const verifiedPrincipal = await host.authentication
        .for(host.audiences[operation])
        .enforce(actor, undefined, ({ principal }) => principal.kind === "user");
      const tenantId = await host.tenantForOwner?.(verifiedPrincipal.subjectId);
      return {
        namespace: host.namespace,
        ownerId: verifiedPrincipal.subjectId,
        ...(tenantId === undefined ? {} : { tenantId }),
      };
    } catch (error) {
      throw safeError(error, "query-failed");
    }
  }

  async function synchronize(target: NotesRepairTarget): Promise<void> {
    if (target.scope.namespace !== host.namespace || !target.scope.ownerId || !target.id)
      throw new Error("Invalid authorized Notes repair target");
    const latest = await host.readLatest(target);
    if (!latest) {
      await host.search.delete(target.scope, { id: target.id, type: "note" });
      return;
    }
    if (latest.id !== target.id || latest.ownerId !== target.scope.ownerId)
      throw new Error("Notes projection read escaped its authorized scope");
    await host.search.upsert(target.scope, {
      id: latest.id,
      type: "note",
      ownerId: target.scope.ownerId,
      ...(target.scope.tenantId === undefined ? {} : { tenantId: target.scope.tenantId }),
      title: latest.title,
      body: latest.body,
    });
  }

  async function afterCommit(target: NotesRepairTarget, result: Note | boolean) {
    try {
      await synchronize(target);
    } catch {
      if (!host.repairTasks)
        throw new NotesProjectionPending(
          result,
          undefined,
          false,
          typeof result === "boolean" ? target.id : undefined,
        );
      let ticket: string;
      try {
        ticket = repairInput.parse({
          ticket: await host.repairTasks.recordRepair(target),
        }).ticket;
      } catch {
        throw new NotesProjectionPending(
          result,
          undefined,
          true,
          typeof result === "boolean" ? target.id : undefined,
        );
      }
      throw new NotesProjectionPending(
        result,
        ticket,
        false,
        typeof result === "boolean" ? target.id : undefined,
      );
    }
  }

  const notes: NotesService = {
    async create(actor, input) {
      const scope = await scopeFor("create", actor);
      const result = await host.notes.create(actor, input);
      await afterCommit({ scope, id: result.id }, result);
      return result;
    },
    list: (actor) => host.notes.list(actor),
    read: (actor, id) => host.notes.read(actor, id),
    async update(actor, id, input) {
      const scope = await scopeFor("update", actor);
      const result = await host.notes.update(actor, id, input);
      if (result) await afterCommit({ scope, id }, result);
      return result;
    },
    async remove(actor, id) {
      const scope = await scopeFor("remove", actor);
      const result = await host.notes.remove(actor, id);
      if (result) await afterCommit({ scope, id }, result);
      return result;
    },
  };

  async function repairTicket(ticket: string): Promise<void> {
    if (!host.repairTasks) throw new SearchError("unsupported-capability");
    const parsed = repairInput.safeParse({ ticket });
    if (!parsed.success) throw new SearchError("invalid-input");
    try {
      await synchronize(await host.repairTasks.authorizeRepair(parsed.data.ticket));
    } catch (error) {
      throw safeError(error, "repair-failed");
    }
  }

  async function repair(actor: NotesActor<"read"> | null, id: string): Promise<void> {
    const parsed = noteLookupInput.safeParse({ id });
    if (!parsed.success) throw new SearchError("invalid-input");
    let scope: NotesRepairTarget["scope"];
    try {
      scope = await scopeFor("read", actor);
      await synchronize({ scope, id: parsed.data.id });
    } catch (error) {
      throw safeError(error, "repair-failed");
    }
  }

  return {
    notes,
    async query(actor: NotesActor<"list"> | null, input: SearchQuery) {
      try {
        const scope = await scopeFor("list", actor);
        if (!input || typeof input !== "object") throw new SearchError("invalid-input");
        // Scope and document type are never accepted from business JSON.
        return await host.search.query(scope, {
          text: input.text,
          pageSize: input.pageSize,
          cursor: input.cursor,
          includeTotal: input.includeTotal,
          sort: input.sort,
          type: "note",
        });
      } catch (error) {
        throw safeError(error, "query-failed");
      }
    },
    createRepairTask(name = "notes.search-repair") {
      if (!host.repairTasks) throw new SearchError("unsupported-capability");
      return defineTask({
        name,
        input: repairInput,
        maxAttempts: 3,
        retry: { delaySeconds: 5, backoff: true, maxDelaySeconds: 60 },
        async handler({ ticket }, context) {
          context.signal.throwIfAborted();
          await repairTicket(ticket);
        },
      });
    },
    repair,
    repairTicket,
  };
}
