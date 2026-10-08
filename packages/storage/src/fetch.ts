import { StorageError } from "./index";
import type { Files, FileUploadInput } from "./files";

export interface FileFetchContext {
  request: Request;
}
type Authenticate<TAccess> = (request: Request) => TAccess | Promise<TAccess>;

async function respond(operation: () => Promise<Response>): Promise<Response> {
  try { return await operation(); } catch (error) {
    if (!(error instanceof StorageError)) throw error;
    const status = {
      "invalid-key": 400, "invalid-input": 400, "not-found": 404, forbidden: 403,
      unsupported: 501, conflict: 409, "too-large": 413, aborted: 499, provider: 502,
    }[error.code];
    return Response.json({ error: error.code }, { status });
  }
}

/** Structurally usable as a Web raw Fetch handler; authentication remains app-owned. */
export function createFileDownloadHandler<TAccess>(options: {
  files: Files<TAccess>;
  authenticate: Authenticate<TAccess>;
  fileId(request: Request): string | null | Promise<string | null>;
}): (context: FileFetchContext) => Promise<Response | undefined> {
  return async ({ request }) => {
    if (!["GET", "HEAD", "DELETE"].includes(request.method)) return undefined;
    const fileId = await options.fileId(request);
    if (fileId === null) return undefined;
    return respond(async () => {
      const access = await options.authenticate(request);
      if (request.method === "DELETE") {
        await options.files.delete(access, fileId);
        return new Response(null, { status: 204 });
      }
      if (request.method === "HEAD") {
        const file = await options.files.metadata(access, fileId);
        if (file.state !== "ready") throw new StorageError("conflict", "File is not ready");
        return new Response(null, { headers: {
          "content-type": file.contentType,
          "content-length": String(file.size),
          ...(file.etag ? { etag: file.etag } : {}),
        } });
      }
      const object = await options.files.read(access, fileId, { signal: request.signal });
      return new Response(object.body, { headers: {
        "content-type": object.metadata.contentType,
        "content-length": String(object.metadata.size),
        ...(object.metadata.etag ? { etag: object.metadata.etag } : {}),
      } });
    });
  };
}

export function createFileUploadHandler<TAccess>(options: {
  files: Files<TAccess>;
  authenticate: Authenticate<TAccess>;
  /** The app chooses storage, ownership, filename and limits, never object keys. */
  input(request: Request, access: TAccess): FileUploadInput | null | Promise<FileUploadInput | null>;
}): (context: FileFetchContext) => Promise<Response | undefined> {
  return async ({ request }) => {
    if (request.method !== "PUT") return undefined;
    return respond(async () => {
      const access = await options.authenticate(request);
      const input = await options.input(request, access);
      if (!input) return new Response(null, { status: 404 });
      if (!request.body) throw new StorageError("invalid-input", "Upload body required");
      const file = await options.files.upload(access, { ...input, body: request.body, signal: request.signal });
      return Response.json(file, { status: 201 });
    });
  };
}
