import { AuthError } from "@lenso/auth";
import { definePlugin, type Plugin } from "@lenso/core";
import { CliError } from "@lenso/cli";
import { defineOperation } from "@lenso/engine/operations";
import { defineManage } from "@lenso/manage";
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
import { notesAudiences, type NotesService } from "./notes";
import type { NotesFileAccess } from "./files";

export interface NotesOperationContext {
  evidence: string | null;
  signal?: AbortSignal;
}

async function safeAuth<T>(work: () => Promise<T>): Promise<T> {
  try {
    return await work();
  } catch (error) {
    if (error instanceof AuthError)
      throw new CliError({
        code: error.code,
        phase: "invoke",
        message: new AuthError(error.code).message,
      });
    throw error;
  }
}

/** Credentials are trusted application configuration, never operation input. */
export function createNotesOperationsService(
  notes: NotesService,
  authentication: NotesAuthentication,
  credential: () => string | null = () => null,
) {
  const fallback = (): NotesOperationContext => ({ evidence: credential() });
  return {
    create(input: z.input<typeof noteInput>, context: NotesOperationContext = fallback()) {
      return safeAuth(async () =>
        notes.create(
          await authentication.for(notesAudiences.create).required(context.evidence, {
            signal: context.signal,
          }),
          input,
        ),
      );
    },
    list(_input: z.input<typeof notesListInput>, context: NotesOperationContext = fallback()) {
      return safeAuth(async () =>
        notes.list(
          await authentication.for(notesAudiences.list).required(context.evidence, {
            signal: context.signal,
          }),
        ),
      );
    },
    read(input: z.input<typeof noteLookupInput>, context: NotesOperationContext = fallback()) {
      return safeAuth(async () =>
        notes.read(
          await authentication.for(notesAudiences.read).required(context.evidence, {
            signal: context.signal,
          }),
          input.id,
        ),
      );
    },
    update(
      { id, ...input }: z.input<typeof noteUpdateInput>,
      context: NotesOperationContext = fallback(),
    ) {
      return safeAuth(async () =>
        notes.update(
          await authentication.for(notesAudiences.update).required(context.evidence, {
            signal: context.signal,
          }),
          id,
          input,
        ),
      );
    },
    remove(input: z.input<typeof noteLookupInput>, context: NotesOperationContext = fallback()) {
      return safeAuth(async () => ({
        removed: await notes.remove(
          await authentication.for(notesAudiences.remove).required(context.evidence, {
            signal: context.signal,
          }),
          input.id,
        ),
      }));
    },
  };
}

export function createNotesOperations(options: {
  notes: Plugin<NotesService>;
  authentication: Plugin<NotesAuthentication>;
  credential?(): string | null;
}) {
  const plugin = definePlugin({
    id: "notes-operations",
    requires: [options.notes, options.authentication],
    setup(context) {
      return createNotesOperationsService(
        context.get(options.notes),
        context.get(options.authentication),
        options.credential,
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
        metadata(
          input: z.input<typeof noteFileInput>,
          context: NotesOperationContext = fallback(),
        ) {
          return safeAuth(async () =>
            files.metadata(
              await authentication.for(notesAudiences.fileMetadata).required(context.evidence, {
                signal: context.signal,
              }),
              input.fileId,
            ),
          );
        },
        delete(input: z.input<typeof noteFileInput>, context: NotesOperationContext = fallback()) {
          return safeAuth(async () =>
            files.delete(
              await authentication.for(notesAudiences.fileDelete).required(context.evidence, {
                signal: context.signal,
              }),
              input.fileId,
            ),
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
