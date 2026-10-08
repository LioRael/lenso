import { constants } from "node:fs";
import { lstat, mkdir, mkdtemp, open, readdir, rename, rm, type FileHandle } from "node:fs/promises";
import { createHash } from "node:crypto";
import { dirname, join, parse, resolve } from "node:path";
import {
  createStoragePlugin, StorageError, unsupportedSigning, validateKey, validateList, validateRange,
  type ObjectMetadata, type PutObjectInput,
} from "./index";
import { checkedUpload } from "./stream";

const objectName = (key: string) => createHash("sha256").update(key).digest("hex");
const temporary = /^\.tmp-[A-Za-z0-9]+$/;
const metadataLimit = 64 * 1024;

function code(error: unknown): string | undefined {
  return (error as NodeJS.ErrnoException)?.code;
}

function storageError(error: unknown): StorageError {
  if (error instanceof StorageError) return error;
  const value = code(error);
  return new StorageError(
    value === "ENOENT" ? "not-found" :
      value === "ELOOP" || value === "EACCES" || value === "EPERM" ? "forbidden" :
        (error as Error)?.name === "AbortError" ? "aborted" : "provider",
    "Local storage operation failed", { cause: error },
  );
}

async function directory(path: string): Promise<void> {
  let stat;
  try { stat = await lstat(path); } catch (error) { throw storageError(error); }
  if (!stat.isDirectory() || stat.isSymbolicLink()) {
    throw new StorageError("forbidden", "Storage directories must not be symlinks");
  }
}

async function ancestry(root: string): Promise<void> {
  const paths: string[] = [];
  for (let path = root; ; path = dirname(path)) {
    paths.unshift(path);
    if (path === parse(path).root) break;
  }
  for (const path of paths) await directory(path);
}

/**
 * Use a trusted, dedicated directory, not a user-writable shared tree.
 * O_NOFOLLOW protects opened files; portable path APIs cannot prevent concurrent
 * hostile writers from swapping ancestor directories between checks and use.
 * Each SHA-256 key directory contains payload and metadata.json. A nonempty
 * directory rename publishes both atomically and cannot replace another object.
 */
export function createLocalStoragePlugin(options: { id: string; root: string }) {
  return createStoragePlugin({
    id: options.id,
    async setup(context) {
      const root = resolve(options.root);
      const lifetime = new AbortController();
      const handles = new Set<FileHandle>();
      const operations = new Set<Promise<unknown>>();
      context.onCleanup(async () => {
        lifetime.abort();
        const results = await Promise.allSettled([...operations]);
        const closed = await Promise.allSettled([...handles].map((handle) => handle.close()));
        handles.clear();
        const failures = [...results, ...closed].filter(
          (result): result is PromiseRejectedResult => result.status === "rejected" &&
            !(result.reason instanceof StorageError && result.reason.code === "aborted" &&
              !(result.reason.cause instanceof AggregateError)),
        );
        if (failures.length) throw new StorageError("provider", "Local storage cleanup failed", {
          cause: new AggregateError(failures.map((failure) => failure.reason)),
        });
      });
      // Create missing directories one component at a time, never through a symlink.
      const paths: string[] = [];
      for (let path = root; path !== parse(path).root; path = dirname(path)) paths.unshift(path);
      await directory(parse(root).root);
      for (const path of paths) {
        try { await mkdir(path, { mode: 0o700 }); }
        catch (error) { if (code(error) !== "EEXIST") throw storageError(error); }
        await directory(path);
      }

      const signalFor = (signal?: AbortSignal) =>
        signal ? AbortSignal.any([signal, lifetime.signal]) : lifetime.signal;
      function run<T>(signal: AbortSignal | undefined, action: (signal: AbortSignal) => Promise<T>): Promise<T> {
        const task = (async () => {
          try {
            const combined = signalFor(signal);
            combined.throwIfAborted();
            await ancestry(root);
            return await action(combined);
          } catch (error) { throw storageError(error); }
        })();
        operations.add(task);
        void task.finally(() => operations.delete(task)).catch(() => {});
        return task;
      }
      async function safeOpen(path: string): Promise<FileHandle> {
        let handle: FileHandle;
        try { handle = await open(path, constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK); }
        catch (error) {
          if (code(error) === "ENOENT") throw new StorageError("provider", "Incomplete stored object", { cause: error });
          throw error;
        }
        handles.add(handle);
        try {
          const stat = await handle.stat();
          if (!stat.isFile() || stat.nlink !== 1) {
            throw new StorageError("forbidden", "Storage files must be regular files without additional hard links");
          }
          return handle;
        } catch (error) { await close(handle, error); throw error; }
      }
      async function close(handle: FileHandle, failure?: unknown): Promise<void> {
        try {
          await handle.close();
          handles.delete(handle);
        } catch (error) {
          throw new StorageError(failure instanceof StorageError ? failure.code : "provider", "Storage file cleanup failed", {
            cause: new AggregateError([...(failure === undefined ? [] : [failure]), error]),
          });
        }
      }
      async function load(name: string, expectedKey?: string) {
        const path = join(root, name);
        await directory(path);
        const identity = await lstat(path);
        const entries = await readdir(path);
        if (entries.length !== 2 || !entries.includes("payload") || !entries.includes("metadata.json")) {
          throw new StorageError("forbidden", "Unexpected object directory entries");
        }
        const metadataFile = await safeOpen(join(path, "metadata.json"));
        let metadata: ObjectMetadata;
        try {
          if ((await metadataFile.stat()).size > metadataLimit) {
            throw new StorageError("provider", "Object metadata exceeds its representation limit");
          }
          const value = JSON.parse(await metadataFile.readFile("utf8"));
          if (typeof value.key !== "string") throw new StorageError("provider", "Invalid stored key");
          validateKey(value.key);
          if (objectName(value.key) !== name || (expectedKey !== undefined && value.key !== expectedKey) ||
              !Number.isSafeInteger(value.size) || value.size < 0 ||
              typeof value.contentType !== "string" || !/^"[a-f0-9]{64}"$/.test(value.etag) ||
              typeof value.lastModified !== "string" || !Number.isFinite(Date.parse(value.lastModified)) ||
              (value.customMetadata !== undefined && (value.customMetadata === null ||
                typeof value.customMetadata !== "object" || Array.isArray(value.customMetadata) ||
                Object.values(value.customMetadata).some((entry) => typeof entry !== "string")))) {
            throw new StorageError("provider", "Invalid stored metadata");
          }
          metadata = { ...value, lastModified: new Date(value.lastModified) };
        } finally { await close(metadataFile); }
        const payload = await safeOpen(join(path, "payload"));
        try {
          if ((await payload.stat()).size !== metadata.size) {
            throw new StorageError("provider", "Stored payload size differs from metadata");
          }
          const current = await lstat(path);
          if (current.dev !== identity.dev || current.ino !== identity.ino || current.isSymbolicLink()) {
            throw new StorageError("conflict", "Object changed while opening its snapshot");
          }
          return { metadata, payload };
        } catch (error) { await close(payload, error); throw error; }
      }
      async function put(input: PutObjectInput, signal: AbortSignal): Promise<ObjectMetadata> {
        validateKey(input.key);
        const contentType = input.contentType ?? "application/octet-stream";
        if (typeof contentType !== "string" ||
            (input.customMetadata !== undefined && (input.customMetadata === null ||
              typeof input.customMetadata !== "object" || Array.isArray(input.customMetadata) ||
              Object.values(input.customMetadata).some((value) => typeof value !== "string")))) {
          throw new StorageError("invalid-input", "Invalid upload metadata");
        }
        const upload = checkedUpload({ ...input, signal });
        let temp: string | undefined;
        let failure: unknown;
        try {
          temp = await mkdtemp(join(root, ".tmp-"));
          const payload = await open(join(temp, "payload"), constants.O_WRONLY | constants.O_CREAT | constants.O_EXCL | constants.O_NOFOLLOW, 0o600);
          handles.add(payload);
          const hash = createHash("sha256");
          const reader = upload.body.getReader();
          try {
            for (;;) {
              const chunk = await reader.read();
              if (chunk.done) break;
              hash.update(chunk.value);
              let offset = 0;
              while (offset < chunk.value.byteLength) {
                signal.throwIfAborted();
                const { bytesWritten } = await payload.write(chunk.value, offset, chunk.value.byteLength - offset);
                if (!bytesWritten) throw new StorageError("provider", "Upload write made no progress");
                offset += bytesWritten;
              }
            }
            await payload.sync();
          } catch (error) {
            failure = storageError(error);
            throw failure;
          } finally { reader.releaseLock(); await close(payload, failure); }
          const metadata: ObjectMetadata = {
            key: input.key, size: upload.size(), contentType,
            etag: `"${hash.digest("hex")}"`, lastModified: new Date(),
            ...(input.customMetadata === undefined ? {} : { customMetadata: { ...input.customMetadata } }),
          };
          const serialized = JSON.stringify(metadata);
          if (Buffer.byteLength(serialized) > metadataLimit) throw new StorageError("invalid-input", "Upload metadata is too large");
          const file = await open(join(temp, "metadata.json"), "wx", 0o600);
          handles.add(file);
          try { await file.writeFile(serialized); await file.sync(); } finally { await close(file); }
          signal.throwIfAborted();
          await ancestry(root);
          const destination = join(root, objectName(input.key));
          try {
            await lstat(destination);
            await directory(destination);
            throw new StorageError("conflict", "Object already exists");
          } catch (error) { if (code(error) !== "ENOENT") throw error; }
          try { await rename(temp, destination); }
          catch (error) {
            if (["EEXIST", "ENOTEMPTY"].includes(code(error) ?? "")) throw new StorageError("conflict", "Object already exists", { cause: error });
            throw error;
          }
          temp = undefined;
          return metadata;
        } catch (error) { failure = storageError(error); throw failure; }
        finally {
          const cleanup = await Promise.allSettled([
            upload.cancel(failure), ...(temp ? [rm(temp, { recursive: true })] : []),
          ]);
          const errors = cleanup.filter((result): result is PromiseRejectedResult => result.status === "rejected");
          if (errors.length) throw new StorageError(failure instanceof StorageError ? failure.code : "provider", "Upload cleanup failed", {
            cause: new AggregateError([...(failure ? [failure] : []), ...errors.map((error) => error.reason)]),
          });
        }
      }
      return {
        capabilities: {
          provider: "local", signedUpload: false, signedDownload: false,
          rangeRead: true, conditionalRead: true, pagination: "key",
          uploadCancellation: "abort", uploadRequiresSize: false,
        },
        put: (input) => run(input.signal, (signal) => put(input, signal)),
        get: (key, options = {}) => run(options.signal, async (signal) => {
          validateKey(key);
          validateRange(options, true);
          const { metadata, payload } = await load(objectName(key), key);
          try {
            if (options.ifMatch !== undefined && options.ifMatch !== metadata.etag) {
              throw new StorageError("conflict", "Object etag does not match");
            }
            const offset = options.range?.offset ?? 0;
            if (options.range && offset >= metadata.size) throw new StorageError("invalid-input", "Range starts outside the object");
            const length = Math.min(options.range?.length ?? metadata.size - offset, metadata.size - offset);
            let position = offset;
            let ended = false;
            let controller: ReadableStreamDefaultController<Uint8Array>;
            const finish = async () => {
              if (ended) return;
              ended = true;
              signal.removeEventListener("abort", abort);
              await close(payload);
            };
            const abort = () => {
              controller.error(new StorageError("aborted", "Download cancelled"));
              void finish().catch(() => {});
            };
            const body = new ReadableStream<Uint8Array>({
              start(value) {
                controller = value;
                signal.addEventListener("abort", abort, { once: true });
                if (signal.aborted) abort();
              },
              async pull(value) {
                try {
                  if (position === offset + length) { await finish(); value.close(); return; }
                  const buffer = new Uint8Array(Math.min(64 * 1024, offset + length - position));
                  const { bytesRead } = await payload.read(buffer, 0, buffer.length, position);
                  if (ended) return;
                  if (!bytesRead) throw new StorageError("provider", "Unexpected end of stored payload");
                  position += bytesRead;
                  value.enqueue(buffer.subarray(0, bytesRead));
                } catch (error) {
                  if (!ended) value.error(storageError(error));
                  await finish();
                }
              },
              cancel: finish,
            }, { highWaterMark: 0 });
            return { metadata, body, ...(options.range ? { range: { offset, length } } : {}) };
          } catch (error) { await close(payload, error); throw error; }
        }),
        head: (key, options = {}) => run(options.signal, async () => {
          validateKey(key);
          try {
            const { metadata, payload } = await load(objectName(key), key);
            await close(payload);
            return metadata;
          } catch (error) {
            if (error instanceof StorageError && error.code === "not-found") return null;
            if (code(error) === "ENOENT") throw new StorageError("provider", "Incomplete stored object", { cause: error });
            throw error;
          }
        }),
        delete: (key, options = {}) => run(options.signal, async (signal) => {
          validateKey(key);
          try {
            const { payload } = await load(objectName(key), key);
            await close(payload);
          } catch (error) {
            if (error instanceof StorageError && error.code === "not-found") return { outcome: "not-found" };
            if (code(error) === "ENOENT") throw new StorageError("provider", "Incomplete stored object", { cause: error });
            throw error;
          }
          signal.throwIfAborted();
          const tomb = await mkdtemp(join(root, ".tmp-"));
          try {
            try { await rename(join(root, objectName(key)), join(tomb, "deleted")); }
            catch (error) { if (code(error) === "ENOENT") return { outcome: "not-found" }; throw error; }
            return { outcome: "deleted" };
          } finally { await rm(tomb, { recursive: true }); }
        }),
        list: (input = {}) => run(input.signal, async (signal) => {
          const limit = validateList(input);
          if (input.cursor !== undefined) validateKey(input.cursor);
          const objects: ObjectMetadata[] = [];
          for (const name of await readdir(root)) {
            signal.throwIfAborted();
            await directory(join(root, name));
            if (temporary.test(name)) continue;
            if (!/^[a-f0-9]{64}$/.test(name)) throw new StorageError("forbidden", "Unexpected storage root entry");
            const { metadata, payload } = await load(name);
            await close(payload);
            if (metadata.key.startsWith(input.prefix ?? "") && (input.cursor === undefined || metadata.key > input.cursor)) objects.push(metadata);
          }
          objects.sort((a, b) => a.key < b.key ? -1 : a.key > b.key ? 1 : 0);
          return { objects: objects.slice(0, limit), ...(objects.length > limit ? { cursor: objects[limit - 1]!.key } : {}) };
        }),
        signUpload: async () => unsupportedSigning(),
        signDownload: async () => unsupportedSigning(),
      };
    },
  });
}
