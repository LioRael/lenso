import { AuthError, authErrorDiagnostic } from "@lenso/auth";
import { definePlugin, type Plugin } from "@lenso/core";
import { defineOperation } from "@lenso/engine/operations";
import { defineManage } from "@lenso/manage";
import { storageErrorDiagnostic } from "@lenso/storage";
import type { Files } from "@lenso/storage/files";
import type { z } from "zod";
import type { NotesAuthentication } from "./auth";
import {
  noteInput,
  noteLookupInput,
  noteUpdateInput,
  notesListInput,
  noteFileInput,
} from "./contracts";
import { notesAudiences, type NotesActor, type NotesService } from "./notes";
import type { NotesFileAccess } from "./files";
import type { AuditService } from "@lenso/audit";

type NotesRemovalAudit = Pick<AuditService<NotesActor<"remove"> | null>, "appendBestEffort">;

export interface NotesOperationContext {
  evidence: string | null;
  signal?: AbortSignal;
}

/** Credentials are trusted application configuration, never operation input. */
export function createNotesOperationsService(
  notes: NotesService,
  authentication: NotesAuthentication,
  credential: () => string | null = () => null,
  audit?: NotesRemovalAudit,
) {
  const fallback = (): NotesOperationContext => ({ evidence: credential() });
  async function recordRemoval(
    actor: NotesActor<"remove">,
    id: string,
    result: "success" | "denied" | "unknown",
    reasonCode: string,
    removed?: boolean,
  ) {
    if (!audit) return;
    await audit.appendBestEffort(
      {
        id: crypto.randomUUID(),
        occurredAt: Date.now(),
        scope: { tenantId: null, scopeId: `owner:${actor.subjectId}` },
        action: "notes.remove",
        target: { type: "note", id },
        result,
        reasonCode,
        summary: removed === undefined ? {} : { removed },
      },
      actor,
    );
  }
  return {
    async create(input: z.input<typeof noteInput>, context: NotesOperationContext = fallback()) {
      return notes.create(
        await authentication.for(notesAudiences.create).required(context.evidence, {
          signal: context.signal,
        }),
        input,
      );
    },
    async list(
      _input: z.input<typeof notesListInput>,
      context: NotesOperationContext = fallback(),
    ) {
      return notes.list(
        await authentication.for(notesAudiences.list).required(context.evidence, {
          signal: context.signal,
        }),
      );
    },
    async read(
      input: z.input<typeof noteLookupInput>,
      context: NotesOperationContext = fallback(),
    ) {
      return notes.read(
        await authentication.for(notesAudiences.read).required(context.evidence, {
          signal: context.signal,
        }),
        input.id,
      );
    },
    async update(
      { id, ...input }: z.input<typeof noteUpdateInput>,
      context: NotesOperationContext = fallback(),
    ) {
      return notes.update(
        await authentication.for(notesAudiences.update).required(context.evidence, {
          signal: context.signal,
        }),
        id,
        input,
      );
    },
    async remove(
      input: z.input<typeof noteLookupInput>,
      context: NotesOperationContext = fallback(),
    ) {
      const actor = await authentication.for(notesAudiences.remove).required(context.evidence, {
        signal: context.signal,
      });
      let removed: boolean;
      try {
        removed = await notes.remove(actor, input.id);
      } catch (error) {
        const denied =
          error instanceof AuthError &&
          (error.code === "UNAUTHORIZED" || error.code === "FORBIDDEN");
        await recordRemoval(
          actor,
          input.id,
          denied ? "denied" : "unknown",
          error instanceof AuthError ? error.code : "operation-unconfirmed",
        );
        throw error;
      }
      await recordRemoval(actor, input.id, "success", removed ? "removed" : "missing", removed);
      return { removed };
    },
  };
}

export function createNotesOperations(options: {
  notes: Plugin<NotesService>;
  authentication: Plugin<NotesAuthentication>;
  credential?(): string | null;
  audit?: Plugin<NotesRemovalAudit>;
}) {
  const plugin = definePlugin({
    id: "notes-operations",
    requires: [options.notes, options.authentication, ...(options.audit ? [options.audit] : [])],
    setup(context) {
      return createNotesOperationsService(
        context.get(options.notes),
        context.get(options.authentication),
        options.credential,
        options.audit ? context.get(options.audit) : undefined,
      );
    },
  });
  const source = { file: "src/operations.ts", export: "createNotesOperationsService" };
  const operations = [
    defineOperation({
      plugin,
      method: "create",
      context: true,
      input: noteInput,
      mapError: authErrorDiagnostic,
      effect: "write",
      destructive: false,
      retry: "unsafe",
      cancellation: "none",
      outputDescription: "The created private note. Repeating creates another note.",
      source,
      description: "Create a private note for the authenticated user.",
    }),
    defineOperation({
      plugin,
      method: "list",
      context: true,
      input: notesListInput,
      mapError: authErrorDiagnostic,
      effect: "read",
      destructive: false,
      retry: "safe",
      cancellation: "none",
      outputDescription: "The authenticated user's private notes.",
      source,
      description: "List the authenticated user's private notes.",
    }),
    defineOperation({
      plugin,
      method: "read",
      context: true,
      input: noteLookupInput,
      mapError: authErrorDiagnostic,
      effect: "read",
      destructive: false,
      retry: "safe",
      cancellation: "none",
      outputDescription: "An owned note, or null if absent.",
      source,
      description: "Read an owned private note.",
    }),
    defineOperation({
      plugin,
      method: "update",
      context: true,
      input: noteUpdateInput,
      mapError: authErrorDiagnostic,
      effect: "write",
      destructive: false,
      retry: "unsafe",
      cancellation: "none",
      outputDescription: "The updated owned note, or null if absent.",
      source,
      description: "Update an owned private note.",
    }),
    defineOperation({
      plugin,
      method: "remove",
      context: true,
      input: noteLookupInput,
      mapError: authErrorDiagnostic,
      effect: "write",
      destructive: true,
      retry: "safe",
      cancellation: "none",
      outputDescription: "removed is true when an owned note was deleted, false if absent.",
      source,
      description: "Remove an owned private note.",
    }),
  ];
  const manage = defineManage({
    plugin,
    operations: operations.filter((operation) =>
      ["list", "read", "remove"].includes(operation.method),
    ),
  });
  return { plugin, operations, manage };
}

export function createNotesFileOperations(options: {
  files: Plugin<Files<NotesFileAccess>>;
  authentication: Plugin<NotesAuthentication>;
  credential?(): string | null;
}) {
  const plugin = definePlugin({
    id: "notes-file-operations",
    requires: [options.files, options.authentication],
    setup(lifecycle) {
      const files = lifecycle.get(options.files);
      const authentication = lifecycle.get(options.authentication);
      const fallback = (): NotesOperationContext => ({ evidence: options.credential?.() ?? null });
      return {
        async metadata(
          input: z.input<typeof noteFileInput>,
          context: NotesOperationContext = fallback(),
        ) {
          return files.metadata(
            await authentication.for(notesAudiences.fileMetadata).required(context.evidence, {
              signal: context.signal,
            }),
            input.fileId,
          );
        },
        async delete(
          input: z.input<typeof noteFileInput>,
          context: NotesOperationContext = fallback(),
        ) {
          return files.delete(
            await authentication.for(notesAudiences.fileDelete).required(context.evidence, {
              signal: context.signal,
            }),
            input.fileId,
          );
        },
      };
    },
  });
  const source = { file: "src/operations.ts", export: "createNotesFileOperations" };
  const operations = [
    defineOperation({
      plugin,
      method: "metadata",
      context: true,
      input: noteFileInput,
      mapError: (error) => authErrorDiagnostic(error) ?? storageErrorDiagnostic(error),
      effect: "read",
      destructive: false,
      retry: "safe",
      cancellation: "none",
      outputDescription:
        "Metadata of the owned file, never binary content or a download credential.",
      source,
      description: "Read metadata of an owned attachment in the local Notes tenant.",
    }),
    defineOperation({
      plugin,
      method: "delete",
      context: true,
      input: noteFileInput,
      mapError: (error) => authErrorDiagnostic(error) ?? storageErrorDiagnostic(error),
      effect: "write",
      destructive: true,
      retry: "safe",
      cancellation: "none",
      outputDescription: "The file record after deletion. Failed deletion remains retryable.",
      source,
      description: "Delete an owned attachment in the local Notes tenant.",
    }),
  ];
  const manage = defineManage({ plugin, operations });
  return { plugin, operations, manage };
}
