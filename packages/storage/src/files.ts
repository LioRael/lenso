import { definePlugin, type Plugin } from "@lenso/core/plugin";
import {
  StorageError,
  validateExpiry,
  validateContentType,
  type ObjectStorage,
  type ObjectMetadata,
  type ObjectDownload,
  type SignedObjectLink,
} from "./index";
import { checkedUpload, validateUpload } from "./stream";

export type FileState = "pending" | "uploading" | "ready" | "failed" | "deleting" | "deleted";
export type FileAction =
  | "upload"
  | "completeUpload"
  | "metadata"
  | "read"
  | "delete"
  | "signDownload";
export interface FileRecord {
  fileId: string;
  storageId: string;
  objectKey: string;
  filename: string;
  contentType: string;
  ownerId: string | null;
  tenantId: string | null;
  state: FileState;
  revision: number;
  size: number | null;
  expectedSize: number | null;
  maxBytes: number | null;
  etag: string | null;
  uploadExpiresAt: number | null;
  createdAt: number;
  updatedAt: number;
}
export interface FileQueries {
  insert(file: FileRecord): Promise<void>;
  get(fileId: string): Promise<FileRecord | null>;
  /** Atomically replace only the matching revision; return false on contention. */
  transition(fileId: string, revision: number, next: FileRecord): Promise<boolean>;
}
export interface FileUploadInput {
  storageId: string;
  filename: string;
  contentType: string;
  size?: number;
  maxBytes?: number;
  ownerId?: string;
  tenantId?: string;
  signal?: AbortSignal;
}
export interface Files<TAccess> {
  upload(
    access: TAccess,
    input: FileUploadInput & { body: ReadableStream<Uint8Array> },
  ): Promise<FileRecord>;
  beginUpload(
    access: TAccess,
    input: FileUploadInput & { maxBytes: number; expiresIn: number },
  ): Promise<{ file: FileRecord; link: SignedObjectLink }>;
  completeUpload(access: TAccess, fileId: string): Promise<FileRecord>;
  metadata(access: TAccess, fileId: string): Promise<FileRecord>;
  read(
    access: TAccess,
    fileId: string,
    options?: { signal?: AbortSignal },
  ): Promise<ObjectDownload>;
  delete(access: TAccess, fileId: string): Promise<FileRecord>;
  signDownload(access: TAccess, fileId: string, expiresIn: number): Promise<SignedObjectLink>;
}

function assertFileReady(file: FileRecord) {
  if (file.state !== "ready") throw new StorageError("conflict", "File is not ready");
}

function verifyUploadedMetadata(file: FileRecord, object: ObjectMetadata | null, direct: boolean) {
  if (!object) throw new StorageError("not-found", "Uploaded object is missing");
  if (
    object.key !== file.objectKey ||
    object.contentType !== file.contentType ||
    !Number.isSafeInteger(object.size) ||
    object.size < 0 ||
    (file.expectedSize !== null && object.size !== file.expectedSize)
  ) {
    throw new StorageError("invalid-input", "Uploaded metadata does not match");
  }
  if (file.maxBytes !== null && object.size > file.maxBytes) {
    throw new StorageError("too-large", "Uploaded object exceeds size limit");
  }
  if (direct && !object.etag)
    throw new StorageError("unsupported", "Direct uploads require an object etag");
  return object;
}

function createFileDraft(input: FileUploadInput): FileRecord {
  validateUpload({ ...input, key: "validation", body: new ReadableStream() });
  validateContentType(input.contentType);
  if (!input.filename) throw new StorageError("invalid-input", "Filename is required");
  const now = Date.now();
  return {
    fileId: crypto.randomUUID(),
    objectKey: `files/${crypto.randomUUID()}`,
    storageId: input.storageId,
    filename: input.filename,
    contentType: input.contentType,
    ownerId: input.ownerId ?? null,
    tenantId: input.tenantId ?? null,
    state: "pending",
    revision: 0,
    size: null,
    etag: null,
    expectedSize: input.size ?? null,
    maxBytes: input.maxBytes ?? null,
    uploadExpiresAt: null,
    createdAt: now,
    updatedAt: now,
  };
}

export function createFilesPlugin<TDb, TAccess>(options: {
  id: string;
  storages: readonly Plugin<ObjectStorage>[];
  database: Plugin<TDb>;
  queries(db: TDb): FileQueries;
  authorize?: (input: {
    access: TAccess;
    action: FileAction;
    file: Readonly<FileRecord>;
  }) => boolean | Promise<boolean>;
}): Plugin<Files<TAccess>> {
  return definePlugin({
    id: options.id,
    requires: [options.database, ...options.storages],
    setup(context) {
      const queries = options.queries(context.get(options.database));
      const storages = new Map(options.storages.map((ref) => [ref.id, context.get(ref)]));
      function storage(file: FileRecord) {
        const result = storages.get(file.storageId);
        if (!result) throw new StorageError("invalid-input", "Unknown storage");
        return result;
      }
      async function authorize(access: TAccess, action: FileAction, file: FileRecord) {
        if (
          !options.authorize ||
          !(await options.authorize({ access, action, file: Object.freeze({ ...file }) }))
        ) {
          throw new StorageError("forbidden", "File operation denied");
        }
      }
      async function load(access: TAccess, action: FileAction, fileId: string) {
        const file = await queries.get(fileId);
        if (!file) throw new StorageError("not-found", "File not found");
        await authorize(access, action, file);
        return file;
      }
      async function transition(file: FileRecord, patch: Partial<FileRecord>) {
        const next = { ...file, ...patch, revision: file.revision + 1, updatedAt: Date.now() };
        if (!(await queries.transition(file.fileId, file.revision, next))) {
          throw new StorageError("conflict", "File state changed concurrently");
        }
        return next;
      }
      async function fail(file: FileRecord, error: unknown, cleanup: boolean): Promise<never> {
        const errors = [error];
        let claimed = false;
        try {
          await transition(file, { state: "failed" });
          claimed = true;
        } catch (cause) {
          errors.push(cause);
          // A lost DB response can follow a committed ready transition. Never
          // destroy its object using the old uploading revision.
          try {
            const current = await queries.get(file.fileId);
            claimed =
              current?.state === "failed" &&
              current.revision === file.revision + 1 &&
              current.storageId === file.storageId &&
              current.objectKey === file.objectKey;
          } catch (lookupError) {
            errors.push(lookupError);
          }
        }
        if (cleanup && claimed) {
          try {
            await storage(file).delete(file.objectKey);
          } catch (cause) {
            errors.push(cause);
          }
        }
        if (errors.length > 1) throw new AggregateError(errors, "Upload and recovery failed");
        throw error;
      }
      return {
        async upload(access, input) {
          let file = createFileDraft(input);
          const objects = storage(file);
          await authorize(access, "upload", file);
          await queries.insert(file);
          file = await transition(file, { state: "uploading" });
          let checked: ReturnType<typeof checkedUpload> | undefined;
          let owned = false;
          try {
            checked = checkedUpload({ ...input, key: file.objectKey });
            // The generated key belongs to this unpublished row, including
            // writes whose provider acknowledgement is lost.
            owned = true;
            const object = await objects.put({ ...input, key: file.objectKey, body: checked.body });
            verifyUploadedMetadata(file, object, false);
            if (object.size !== checked.size())
              throw new StorageError("invalid-input", "Adapter did not consume the upload");
            await checked.cancel();
            return await transition(file, {
              state: "ready",
              size: object.size,
              etag: object.etag ?? null,
            });
          } catch (error) {
            if (
              error instanceof StorageError &&
              (error.code === "conflict" || error.code === "forbidden")
            ) {
              owned = false;
            }
            let failure = error;
            try {
              await checked?.cancel(error);
            } catch (cause) {
              failure = new AggregateError([error, cause], "Upload and source cleanup failed");
            }
            return await fail(file, failure, owned);
          }
        },
        async beginUpload(access, input) {
          validateExpiry(input.expiresIn);
          if (input.maxBytes === undefined)
            throw new StorageError("invalid-input", "Direct upload size limit is required");
          let file = createFileDraft(input);
          const objects = storage(file);
          await authorize(access, "upload", file);
          if (!objects.capabilities.signedUpload || !objects.capabilities.conditionalRead) {
            throw new StorageError(
              "unsupported",
              "Safe direct uploads require signing and conditional reads",
            );
          }
          await queries.insert(file);
          try {
            const link = await objects.signUpload({
              key: file.objectKey,
              expiresIn: input.expiresIn,
              contentType: file.contentType,
              maxBytes: input.maxBytes,
            });
            if (
              link.method !== "PUT" ||
              !link.conditions.createOnly ||
              !Number.isFinite(link.expiresAt.getTime()) ||
              link.expiresAt.getTime() <= Date.now() ||
              link.expiresAt.getTime() > Date.now() + input.expiresIn * 1000
            ) {
              throw new StorageError(
                "unsupported",
                "Adapter did not issue a bounded create-only upload",
              );
            }
            file = await transition(file, { uploadExpiresAt: link.expiresAt.getTime() });
            return { file, link };
          } catch (error) {
            return await fail(file, error, false);
          }
        },
        async completeUpload(access, fileId) {
          let file = await load(access, "completeUpload", fileId);
          if (file.state !== "pending" || file.uploadExpiresAt === null) {
            throw new StorageError("conflict", "File is not awaiting direct completion");
          }
          file = await transition(file, { state: "uploading" });
          try {
            const object = verifyUploadedMetadata(
              file,
              await storage(file).head(file.objectKey),
              true,
            );
            return await transition(file, {
              state: "ready",
              size: object.size,
              etag: object.etag!,
            });
          } catch (error) {
            return await fail(file, error, true);
          }
        },
        async metadata(access, fileId) {
          return load(access, "metadata", fileId);
        },
        async read(access, fileId, readOptions) {
          const file = await load(access, "read", fileId);
          assertFileReady(file);
          const objects = storage(file);
          return objects.get(file.objectKey, {
            ...readOptions,
            ifMatch: objects.capabilities.conditionalRead ? (file.etag ?? undefined) : undefined,
          });
        },
        async delete(access, fileId) {
          let file = await load(access, "delete", fileId);
          if (file.state === "deleted") return file;
          if (file.state === "uploading")
            throw new StorageError("conflict", "Upload is in progress");
          // A live PUT credential can recreate a removed object. Retry after expiry.
          if (file.uploadExpiresAt !== null && Date.now() < file.uploadExpiresAt) {
            throw new StorageError(
              "conflict",
              "Wait for the upload credential to expire before deleting",
            );
          }
          file = await transition(file, { state: "deleting" });
          await storage(file).delete(file.objectKey);
          return transition(file, { state: "deleted" });
        },
        async signDownload(access, fileId, expiresIn) {
          validateExpiry(expiresIn);
          const file = await load(access, "signDownload", fileId);
          assertFileReady(file);
          const link = await storage(file).signDownload({
            key: file.objectKey,
            expiresIn,
            ifMatch: file.etag ?? undefined,
          });
          if (file.etag && link.conditions.ifMatch !== file.etag) {
            throw new StorageError(
              "unsupported",
              "Signed download cannot pin the published object",
            );
          }
          return link;
        },
      };
    },
  });
}
