import { AuthError } from "@lenso/auth";
import { definePlugin, type Plugin } from "@lenso/core";
import { CliError, defineOperation } from "@lenso/cli";
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
  credential: () => string | null,
) {
  return {
    create(input: z.input<typeof noteInput>) {
      return safeAuth(async () =>
        notes.create(await authentication.for(notesAudiences.create).required(credential()), input),
      );
    },
    list(_input: z.input<typeof notesListInput>) {
      return safeAuth(async () =>
        notes.list(await authentication.for(notesAudiences.list).required(credential())),
      );
    },
    read(input: z.input<typeof noteLookupInput>) {
      return safeAuth(async () =>
        notes.read(await authentication.for(notesAudiences.read).required(credential()), input.id),
      );
    },
    update({ id, ...input }: z.input<typeof noteUpdateInput>) {
      return safeAuth(async () =>
        notes.update(
          await authentication.for(notesAudiences.update).required(credential()),
          id,
          input,
        ),
      );
    },
    remove(input: z.input<typeof noteLookupInput>) {
      return safeAuth(async () => ({
        removed: await notes.remove(
          await authentication.for(notesAudiences.remove).required(credential()),
          input.id,
        ),
      }));
    },
  };
}

export function createNotesOperationsPlugin(options: {
  notes: Plugin<NotesService>;
  authentication: Plugin<NotesAuthentication>;
  credential(): string | null;
}) {
  return definePlugin({
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
}

export function declareNotesOperations(plugin: ReturnType<typeof createNotesOperationsPlugin>) {
  const source = { file: "src/operations.ts", export: "createNotesOperationsService" };
  return [
    defineOperation({
      plugin,
      method: "create",
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
}

export function createNotesFileOperationsPlugin(options: {
  files: Plugin<Files<NotesFileAccess>>;
  authentication: Plugin<NotesAuthentication>;
  credential(): string | null;
}) {
  return definePlugin({
    id: "notes-file-operations",
    requires: [options.files, options.authentication],
    setup(context) {
      const files = context.get(options.files);
      const authentication = context.get(options.authentication);
      return {
        metadata(input: z.input<typeof noteFileInput>) {
          return safeAuth(async () =>
            files.metadata(
              await authentication.for(notesAudiences.fileMetadata).required(options.credential()),
              input.fileId,
            ),
          );
        },
        delete(input: z.input<typeof noteFileInput>) {
          return safeAuth(async () =>
            files.delete(
              await authentication.for(notesAudiences.fileDelete).required(options.credential()),
              input.fileId,
            ),
          );
        },
      };
    },
  });
}

export function declareNotesFileOperations(
  plugin: ReturnType<typeof createNotesFileOperationsPlugin>,
) {
  const source = { file: "src/operations.ts", export: "createNotesFileOperationsPlugin" };
  return [
    defineOperation({
      plugin,
      method: "metadata",
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
}
