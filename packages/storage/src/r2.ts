import {
  createStoragePlugin,
  StorageError,
  unsupportedSigning,
  validateKey,
  validateList,
  validateRange,
  type ObjectMetadata,
  type ObjectStorage,
} from "./index";
import { checkedUpload, validateUpload } from "./stream";

/** Structural subset of the native binding; no Workers globals are installed by this module. */
export interface R2StorageObject {
  key: string;
  size: number;
  etag: string;
  httpEtag: string;
  version: string;
  uploaded: Date;
  httpMetadata?: { contentType?: string };
  customMetadata?: Record<string, string>;
  body?: ReadableStream<Uint8Array>;
  range?: { offset?: number; length?: number; suffix?: number };
}

export interface R2StorageBinding {
  put(
    key: string,
    body: ReadableStream<Uint8Array>,
    options: {
      onlyIf: Headers;
      httpMetadata: { contentType: string };
      customMetadata?: Record<string, string>;
    },
  ): Promise<R2StorageObject | null>;
  get(
    key: string,
    options?: {
      onlyIf?: Headers;
      range?: { offset: number; length?: number };
    },
  ): Promise<R2StorageObject | null>;
  head(key: string): Promise<R2StorageObject | null>;
  delete(key: string): Promise<void>;
  list(options?: {
    prefix?: string;
    cursor?: string;
    limit?: number;
    include?: ("httpMetadata" | "customMetadata")[];
  }): Promise<{ objects: R2StorageObject[]; truncated: boolean; cursor?: string }>;
}

type FixedLengthStreamConstructor = new (size: number) => {
  readable: ReadableStream<Uint8Array>;
  writable: WritableStream<Uint8Array>;
};

function metadata(object: R2StorageObject): ObjectMetadata {
  return {
    key: object.key,
    size: object.size,
    contentType: object.httpMetadata?.contentType ?? "application/octet-stream",
    etag: object.httpEtag,
    versionId: object.version,
    lastModified: object.uploaded,
    customMetadata: object.customMetadata,
  };
}

function failure(error: unknown, signal?: AbortSignal): StorageError {
  if (error instanceof StorageError) return error;
  if (signal?.aborted)
    return new StorageError("aborted", "Storage operation cancelled", { cause: error });
  const value = error as {
    status?: number;
    statusCode?: number;
    code?: number | string;
    name?: string;
  } | null;
  const status = value?.status ?? value?.statusCode ?? value?.code;
  const code =
    status === 403 || status === 10003 || value?.name === "AccessDenied"
      ? "forbidden"
      : status === 404 || value?.name === "NoSuchKey"
        ? "not-found"
        : status === 409 || status === 412
          ? "conflict"
          : "provider";
  return new StorageError(code, "R2 storage operation failed", { cause: error });
}

function checkSignal(signal?: AbortSignal): void {
  if (signal?.aborted)
    throw new StorageError("aborted", "Storage operation cancelled", { cause: signal.reason });
}

export function createR2StoragePlugin(options: { id: string; binding: R2StorageBinding }) {
  return createStoragePlugin({
    id: options.id,
    setup() {
      const binding = options.binding;
      return {
        capabilities: {
          provider: "r2-binding",
          signedUpload: false,
          signedDownload: false,
          rangeRead: true,
          conditionalRead: true,
          pagination: "opaque",
          uploadCancellation: "stream-only",
          uploadRequiresSize: true,
        },
        async put(input) {
          validateKey(input.key);
          checkSignal(input.signal);
          validateUpload(input);
          if (input.size === undefined)
            throw new StorageError("invalid-input", "R2 binding uploads require size");
          const FixedLengthStream = (
            globalThis as unknown as {
              FixedLengthStream?: FixedLengthStreamConstructor;
            }
          ).FixedLengthStream;
          if (!FixedLengthStream)
            throw new StorageError(
              "unsupported",
              "R2 uploads require the Workers FixedLengthStream runtime",
            );
          const fixed = new FixedLengthStream(input.size);
          const upload = checkedUpload(input);
          const controller = new AbortController();
          const abort = () => controller.abort(input.signal?.reason);
          input.signal?.addEventListener("abort", abort, { once: true });
          if (input.signal?.aborted) abort();
          const pumping = upload.body.pipeTo(fixed.writable, { signal: controller.signal });
          // Observe pipe failure immediately, even if the binding stops reading on a failed condition.
          void pumping.catch(() => {});
          try {
            const object = await binding.put(input.key, fixed.readable, {
              onlyIf: new Headers({ "If-None-Match": "*" }),
              httpMetadata: { contentType: input.contentType ?? "application/octet-stream" },
              customMetadata: input.customMetadata,
            });
            if (!object) throw new StorageError("conflict", "Object already exists");
            await pumping;
            // A cancellation racing successful commit must not delete the object.
            return metadata(object);
          } catch (error) {
            controller.abort();
            try {
              await upload.cancel(error);
            } catch (cleanupError) {
              await pumping.catch(() => {});
              throw new StorageError("provider", "R2 upload failed and source cleanup failed", {
                cause: new AggregateError([error, cleanupError], "Upload and cleanup failed"),
              });
            }
            const pumpError = await pumping.catch((value: unknown) => value);
            if (pumpError instanceof StorageError) throw failure(pumpError, input.signal);
            if (upload.size() > input.size) {
              throw new StorageError("invalid-input", "Upload size differs from declared size", {
                cause: error,
              });
            }
            throw failure(error, input.signal);
          } finally {
            input.signal?.removeEventListener("abort", abort);
          }
        },
        async get(key, input = {}) {
          validateKey(key);
          validateRange(input, true);
          checkSignal(input.signal);
          try {
            const object = await binding.get(key, {
              range: input.range,
              onlyIf:
                input.ifMatch === undefined
                  ? undefined
                  : new Headers({ "If-Match": input.ifMatch }),
            });
            if (!object) throw new StorageError("not-found", "Object not found");
            if (!object.body) throw new StorageError("conflict", "Object condition failed");
            return {
              metadata: metadata(object),
              body: object.body,
              range: input.range
                ? {
                    offset: object.range?.offset ?? input.range.offset,
                    length:
                      object.range?.length ??
                      Math.max(
                        0,
                        Math.min(
                          input.range.length ?? object.size,
                          object.size - input.range.offset,
                        ),
                      ),
                  }
                : undefined,
            };
          } catch (error) {
            throw failure(error, input.signal);
          }
        },
        async head(key, input = {}) {
          validateKey(key);
          checkSignal(input.signal);
          try {
            const object = await binding.head(key);
            return object ? metadata(object) : null;
          } catch (error) {
            throw failure(error, input.signal);
          }
        },
        async delete(key, input = {}) {
          validateKey(key);
          checkSignal(input.signal);
          try {
            await binding.delete(key);
            return { outcome: "absent-or-deleted" };
          } catch (error) {
            throw failure(error, input.signal);
          }
        },
        async list(input = {}) {
          const limit = validateList(input);
          checkSignal(input.signal);
          try {
            const page = await binding.list({
              prefix: input.prefix,
              cursor: input.cursor,
              limit,
              include: ["httpMetadata", "customMetadata"],
            });
            return {
              objects: page.objects.map(metadata),
              cursor: page.truncated ? page.cursor : undefined,
            };
          } catch (error) {
            throw failure(error, input.signal);
          }
        },
        async signUpload() {
          return unsupportedSigning();
        },
        async signDownload() {
          return unsupportedSigning();
        },
      } satisfies Omit<ObjectStorage, "id">;
    },
  });
}
