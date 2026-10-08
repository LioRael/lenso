import {
  AbortMultipartUploadCommand,
  CompleteMultipartUploadCommand,
  DeleteObjectCommand,
  CreateMultipartUploadCommand,
  GetObjectCommand,
  HeadObjectCommand,
  ListObjectsV2Command,
  PutObjectCommand,
  S3Client,
  type S3ClientConfig,
  type HeadObjectCommandOutput,
} from "@aws-sdk/client-s3";
import { Readable } from "node:stream";
import { finished } from "node:stream/promises";
import { Upload } from "@aws-sdk/lib-storage";
import { getSignedUrl } from "@aws-sdk/s3-request-presigner";
import {
  createStoragePlugin,
  StorageError,
  validateExpiry,
  validateKey,
  validateList,
  validateRange,
  type ObjectMetadata,
  type ObjectStorage,
} from "./index";
import { checkedUpload, validateUpload } from "./stream";

export interface S3StorageOptions {
  id: string;
  bucket: string;
  client?: S3Client;
  clientConfig?: S3ClientConfig;
  /** Disable multipart for endpoints without conditional CompleteMultipartUpload. Requires size <= 5 GiB. */
  multipart?: boolean;
}

function failure(error: unknown, signal?: AbortSignal): StorageError {
  if (error instanceof StorageError) return error;
  if (signal?.aborted)
    return new StorageError("aborted", "Storage operation cancelled", { cause: error });
  const value = error as { $metadata?: { httpStatusCode?: number }; name?: string } | null;
  const status = value?.$metadata?.httpStatusCode;
  const code =
    status === 403 || value?.name === "AccessDenied"
      ? "forbidden"
      : status === 404 || value?.name === "NoSuchKey" || value?.name === "NotFound"
        ? "not-found"
        : status === 409 || status === 412
          ? "conflict"
          : value?.name === "AbortError"
            ? "aborted"
            : "provider";
  return new StorageError(code, "S3 storage operation failed", { cause: error });
}

function metadata(key: string, object: HeadObjectCommandOutput): ObjectMetadata {
  return {
    key,
    size: object.ContentLength ?? 0,
    contentType: object.ContentType ?? "application/octet-stream",
    etag: object.ETag,
    versionId: object.VersionId,
    lastModified: object.LastModified,
    customMetadata: object.Metadata,
  };
}

/**
 * Requires native If-None-Match on PutObject AND CompleteMultipartUpload.
 * Compatible endpoints must enforce these conditions, not merely accept the headers.
 * R2 documents conditional PutObject but not conditional multipart completion.
 */
export function createS3StoragePlugin(options: S3StorageOptions) {
  return createStoragePlugin({
    id: options.id,
    async setup(context) {
      const client =
        options.client ??
        new S3Client({ requestChecksumCalculation: "WHEN_REQUIRED", ...options.clientConfig });
      if (!options.client) context.onCleanup(() => client.destroy());
      const active = new Map<AbortController, Promise<unknown>>();
      let stopped = false;
      context.onCleanup(async () => {
        stopped = true;
        for (const controller of active.keys()) controller.abort();
        await Promise.allSettled(active.values());
      });
      const endpoint = await client.config.endpoint?.();
      const r2Endpoint = endpoint?.hostname.endsWith(".r2.cloudflarestorage.com") ?? false;
      const multipart = !r2Endpoint && options.multipart !== false;
      const Bucket = options.bucket;
      const service: Omit<ObjectStorage, "id"> = {
        capabilities: {
          provider: "s3",
          signedUpload: true,
          signedDownload: true,
          rangeRead: true,
          conditionalRead: true,
          pagination: "opaque",
          uploadCancellation: "abort",
          uploadRequiresSize: !multipart,
        },
        async put(input) {
          validateKey(input.key);
          if (stopped) throw new StorageError("aborted", "Storage plugin stopped");
          try {
            validateUpload(input);
          } catch (error) {
            throw failure(error, input.signal);
          }
          if (!multipart && input.size === undefined)
            throw new StorageError("invalid-input", "Single PUT uploads require size");
          if (!multipart && input.size! > 5 * 1024 ** 3)
            throw new StorageError("too-large", "Single PUT uploads cannot exceed 5 GiB");
          if (
            !multipart &&
            (await client.config.requestChecksumCalculation()) !== "WHEN_REQUIRED"
          ) {
            throw new StorageError(
              "unsupported",
              "Single PUT streaming requires requestChecksumCalculation WHEN_REQUIRED",
            );
          }
          const controller = new AbortController();
          const abort = () => controller.abort(input.signal?.reason);
          input.signal?.addEventListener("abort", abort, { once: true });
          if (input.signal?.aborted) abort();
          const upload = checkedUpload({ ...input, signal: controller.signal });
          let uploadId: string | undefined;
          // Scope request cancellation and condition guards to this upload, never mutate an external client.
          const uploadClient = new Proxy(client, {
            get(target, property, receiver) {
              if (property !== "send") return Reflect.get(target, property, receiver);
              return async (
                command: { input: Record<string, unknown> },
                requestOptions?: object,
              ) => {
                if (
                  command instanceof PutObjectCommand ||
                  command instanceof CompleteMultipartUploadCommand
                ) {
                  if (command.input.IfNoneMatch !== "*") {
                    throw new StorageError(
                      "unsupported",
                      "SDK must preserve create-only upload conditions",
                    );
                  }
                  if (r2Endpoint && command instanceof CompleteMultipartUploadCommand) {
                    throw new StorageError(
                      "unsupported",
                      "R2 S3 conditional multipart completion is not documented; use the R2 binding",
                    );
                  }
                }
                // Unlike Upload.abort(), request abort waits for lib-storage's failure cleanup path.
                const result = await Reflect.apply(target.send, target, [
                  command,
                  {
                    ...requestOptions,
                    ...(command instanceof AbortMultipartUploadCommand
                      ? {}
                      : { abortSignal: controller.signal }),
                  },
                ]);
                if (command instanceof CreateMultipartUploadCommand) uploadId = result.UploadId;
                if (
                  command instanceof AbortMultipartUploadCommand ||
                  command instanceof CompleteMultipartUploadCommand
                )
                  uploadId = undefined;
                return result;
              };
            },
          });
          const task = (async () => {
            let body: Readable | undefined;
            try {
              if (!multipart) {
                body = Readable.from(
                  (async function* () {
                    const reader = upload.body.getReader();
                    try {
                      while (true) {
                        const chunk = await reader.read();
                        if (chunk.done) break;
                        yield chunk.value;
                      }
                    } finally {
                      reader.releaseLock();
                    }
                  })(),
                  { objectMode: false },
                );
              }
              const result = multipart
                ? await new Upload({
                    client: uploadClient,
                    params: {
                      Bucket,
                      Key: input.key,
                      Body: upload.body,
                      ContentType: input.contentType ?? "application/octet-stream",
                      Metadata: input.customMetadata,
                      IfNoneMatch: "*",
                    },
                    queueSize: 1,
                    partSize: 5 * 1024 * 1024,
                    leavePartsOnError: false,
                  }).done()
                : await client.send(
                    new PutObjectCommand({
                      Bucket,
                      Key: input.key,
                      Body: body,
                      ContentLength: input.size,
                      ContentType: input.contentType ?? "application/octet-stream",
                      Metadata: input.customMetadata,
                      IfNoneMatch: "*",
                    }),
                    { abortSignal: controller.signal },
                  );
              if (body) await finished(body, { cleanup: true });
              return {
                key: input.key,
                size: upload.size(),
                contentType: input.contentType ?? "application/octet-stream",
                etag: result.ETag,
                versionId: result.VersionId,
                customMetadata: input.customMetadata,
              };
            } catch (error) {
              body?.destroy();
              const cleanupErrors: unknown[] = [];
              try {
                await upload.cancel(error);
              } catch (cleanupError) {
                cleanupErrors.push(cleanupError);
              }
              // lib-storage cleans part failures, but its completion failure path leaves parts behind.
              if (uploadId) {
                try {
                  await client.send(
                    new AbortMultipartUploadCommand({ Bucket, Key: input.key, UploadId: uploadId }),
                  );
                } catch (cleanupError) {
                  cleanupErrors.push(cleanupError);
                }
              }
              if (cleanupErrors.length)
                throw new StorageError("provider", "S3 upload failed and cleanup failed", {
                  cause: new AggregateError([error, ...cleanupErrors], "Upload and cleanup failed"),
                });
              // Failed/ambiguous completion can already have committed. Never issue DeleteObject here.
              throw failure(error, controller.signal);
            } finally {
              input.signal?.removeEventListener("abort", abort);
              active.delete(controller);
            }
          })();
          active.set(controller, task);
          return task;
        },
        async head(key, input = {}) {
          validateKey(key);
          try {
            const result = await client.send(new HeadObjectCommand({ Bucket, Key: key }), {
              abortSignal: input.signal,
            });
            return metadata(key, result);
          } catch (error) {
            const mapped = failure(error, input.signal);
            if (mapped.code === "not-found") return null;
            throw mapped;
          }
        },
        async get(key, input = {}) {
          validateKey(key);
          validateRange(input, true);
          try {
            const range = input.range;
            const result = await client.send(
              new GetObjectCommand({
                Bucket,
                Key: key,
                IfMatch: input.ifMatch,
                Range: range
                  ? `bytes=${range.offset}-${range.length === undefined ? "" : range.offset + range.length - 1}`
                  : undefined,
              }),
              { abortSignal: input.signal },
            );
            if (!result.Body) throw new StorageError("provider", "S3 response has no body");
            const matched = result.ContentRange?.match(/^bytes (\d+)-(\d+)\/(\d+)$/);
            return {
              metadata: metadata(key, {
                ...result,
                ContentLength: matched ? Number(matched[3]) : result.ContentLength,
              }),
              body: result.Body.transformToWebStream() as ReadableStream<Uint8Array>,
              range: matched
                ? {
                    offset: Number(matched[1]),
                    length: Number(matched[2]) - Number(matched[1]) + 1,
                  }
                : undefined,
            };
          } catch (error) {
            throw failure(error, input.signal);
          }
        },
        async delete(key, input = {}) {
          validateKey(key);
          try {
            await client.send(new DeleteObjectCommand({ Bucket, Key: key }), {
              abortSignal: input.signal,
            });
            return { outcome: "absent-or-deleted" };
          } catch (error) {
            throw failure(error, input.signal);
          }
        },
        async list(input = {}) {
          const limit = validateList(input);
          try {
            const page = await client.send(
              new ListObjectsV2Command({
                Bucket,
                Prefix: input.prefix,
                ContinuationToken: input.cursor,
                MaxKeys: limit,
              }),
              { abortSignal: input.signal },
            );
            const objects: ObjectMetadata[] = [];
            // ListObjectsV2 omits content type. Serial HEADs bound concurrency and report current metadata.
            for (const item of page.Contents ?? []) {
              if (!item.Key) continue;
              const object = await service.head(item.Key, { signal: input.signal });
              if (object) objects.push(object);
            }
            return { objects, cursor: page.IsTruncated ? page.NextContinuationToken : undefined };
          } catch (error) {
            throw failure(error, input.signal);
          }
        },
        async signUpload(input) {
          validateKey(input.key);
          validateExpiry(input.expiresIn);
          validateUpload({ key: input.key, body: new ReadableStream(), maxBytes: input.maxBytes });
          try {
            // A caller's default checksum policy would sign CRC32 of an empty, not-yet-uploaded body.
            const signingClient = new Proxy(client, {
              get(target, property, receiver) {
                if (property === "config") {
                  return {
                    ...target.config,
                    requestChecksumCalculation: async () => "WHEN_REQUIRED",
                  };
                }
                return Reflect.get(target, property, receiver);
              },
            });
            const url = await getSignedUrl(
              signingClient,
              new PutObjectCommand({
                Bucket,
                Key: input.key,
                ContentType: input.contentType,
                IfNoneMatch: "*",
              }),
              {
                expiresIn: input.expiresIn,
                signableHeaders: new Set(["content-type", "if-none-match"]),
              },
            );
            return {
              url,
              method: "PUT",
              headers: { "content-type": input.contentType, "if-none-match": "*" },
              expiresAt: new Date(Date.now() + input.expiresIn * 1000),
              conditions: {
                contentType: input.contentType,
                createOnly: true,
                maxBytes: input.maxBytes,
                sizeEnforcement: input.maxBytes === undefined ? "none" : "completion-check",
              },
            };
          } catch (error) {
            throw failure(error);
          }
        },
        async signDownload(input) {
          validateKey(input.key);
          validateExpiry(input.expiresIn);
          try {
            const url = await getSignedUrl(
              client,
              new GetObjectCommand({
                Bucket,
                Key: input.key,
                IfMatch: input.ifMatch,
              }),
              { expiresIn: input.expiresIn, signableHeaders: new Set(["if-match"]) },
            );
            const headers: Record<string, string> = {};
            if (input.ifMatch !== undefined) headers["if-match"] = input.ifMatch;
            return {
              url,
              method: "GET",
              headers,
              expiresAt: new Date(Date.now() + input.expiresIn * 1000),
              conditions: { ifMatch: input.ifMatch, sizeEnforcement: "none" },
            };
          } catch (error) {
            throw failure(error);
          }
        },
      };
      return service;
    },
  });
}
