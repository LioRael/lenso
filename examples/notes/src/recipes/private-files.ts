import { bearerEvidence } from "@lenso/auth/fetch";
import { createFileDownloadHandler, createFileUploadHandler } from "@lenso/storage/fetch";
import type { Files } from "@lenso/storage/files";
import type { WebContext } from "@lenso/web";
import type { NotesAuthentication } from "../auth";
import { notesFileTenant, type NotesFileAccess } from "../private-files";
import { notesAudiences } from "../notes";
import { RecipeInputError } from "./login";

export const filesPath = "/files";
export const maxUploadBytes = 1024 * 1024;

/** Host chooses the storage instance and limits. The client never chooses owner, tenant or key. */
export function createPrivateFileRoutes(options: {
  auth: NotesAuthentication;
  files: Files<NotesFileAccess>;
  storageId: string;
}) {
  const authenticate = async (request: Request) => {
    const evidence = bearerEvidence({ request });
    const audience =
      request.method === "PUT"
        ? notesAudiences.create
        : request.method === "DELETE"
          ? notesAudiences.fileDelete
          : notesAudiences.read;
    return options.auth.for(audience).required(evidence.evidence, evidence);
  };
  const upload = createFileUploadHandler({
    files: options.files,
    authenticate,
    input(request, access) {
      const length = request.headers.get("content-length");
      const size = length === null ? undefined : Number(length);
      if (size !== undefined && (!/^\d+$/.test(length!) || !Number.isSafeInteger(size)))
        throw new RecipeInputError("Invalid Content-Length");
      return {
        storageId: options.storageId,
        filename: request.headers.get("x-file-name") ?? "attachment.bin",
        contentType: request.headers.get("content-type") ?? "application/octet-stream",
        ownerId: access.subjectId,
        tenantId: notesFileTenant,
        size,
        maxBytes: maxUploadBytes,
      };
    },
  });
  const download = createFileDownloadHandler({
    files: options.files,
    authenticate,
    fileId(request) {
      return /^\/files\/([^/]+)$/.exec(new URL(request.url).pathname)?.[1] ?? null;
    },
  });
  return async (context: WebContext) => {
    const path = new URL(context.request.url).pathname;
    const collection = path === filesPath;
    if (!collection && !/^\/files\/[^/]+$/.test(path)) return undefined;
    const allowed = collection ? ["PUT"] : ["GET", "HEAD", "DELETE"];
    if (!allowed.includes(context.request.method))
      return new Response(null, { status: 405, headers: { allow: allowed.join(", ") } });
    return collection ? upload(context) : download(context);
  };
}
