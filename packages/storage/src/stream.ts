import { StorageError, validateContentType, type PutObjectInput } from "./index";

export function validateUpload(input: PutObjectInput): void {
  if (input.contentType !== undefined) validateContentType(input.contentType);
  if (
    input.customMetadata !== undefined &&
    (input.customMetadata === null ||
      typeof input.customMetadata !== "object" ||
      Array.isArray(input.customMetadata) ||
      Object.values(input.customMetadata).some((value) => typeof value !== "string"))
  ) {
    throw new StorageError("invalid-input", "Custom metadata must be a string record");
  }
  for (const [name, value] of [
    ["size", input.size],
    ["maxBytes", input.maxBytes],
  ] as const) {
    if (value !== undefined && (!Number.isSafeInteger(value) || value < 0)) {
      throw new StorageError("invalid-input", `${name} must be a nonnegative safe integer`);
    }
  }
  if (input.size !== undefined && input.maxBytes !== undefined && input.size > input.maxBytes) {
    throw new StorageError("too-large", "Upload exceeds the size limit");
  }
  input.signal?.throwIfAborted();
}

/** Keep provider reads pull-based and interrupt pending reads after get() resolves. */
export function checkedDownload(
  source: ReadableStream<Uint8Array>,
  signal: AbortSignal | undefined,
  mapError: (error: unknown) => StorageError,
): ReadableStream<Uint8Array> {
  const reader = source.getReader();
  let ended = false;
  let controller: ReadableStreamDefaultController<Uint8Array>;
  const detach = () => signal?.removeEventListener("abort", abort);
  async function cancel(reason?: unknown) {
    if (ended) return;
    ended = true;
    detach();
    try {
      await reader.cancel(reason);
    } catch (error) {
      throw mapError(error);
    } finally {
      reader.releaseLock();
    }
  }
  function abort() {
    const error = new StorageError("aborted", "Download cancelled", { cause: signal?.reason });
    controller.error(error);
    void cancel(error).catch(() => {});
  }
  return new ReadableStream<Uint8Array>(
    {
      start(value) {
        controller = value;
        signal?.addEventListener("abort", abort, { once: true });
        if (signal?.aborted) abort();
      },
      async pull(value) {
        try {
          const chunk = await reader.read();
          if (ended) return;
          if (chunk.done) {
            ended = true;
            detach();
            reader.releaseLock();
            value.close();
          } else value.enqueue(chunk.value);
        } catch (error) {
          if (ended) return;
          ended = true;
          detach();
          reader.releaseLock();
          value.error(mapError(error));
        }
      },
      cancel,
    },
    { highWaterMark: 0 },
  );
}

/** Pull-based counting also interrupts a source blocked in read() on cancellation. */
export function checkedUpload(input: PutObjectInput): {
  body: ReadableStream<Uint8Array>;
  size(): number;
  cancel(reason?: unknown): Promise<void>;
} {
  validateUpload(input);
  const reader = input.body.getReader();
  let bytes = 0;
  let ended = false;
  let cancellation: Promise<void> | undefined;
  let controller: ReadableStreamDefaultController<Uint8Array>;
  const detach = () => input.signal?.removeEventListener("abort", abort);
  function cancel(reason?: unknown): Promise<void> {
    if (cancellation) return cancellation;
    if (ended) return Promise.resolve();
    ended = true;
    detach();
    cancellation = reader
      .cancel(reason)
      .catch((error: unknown) => {
        // An already errored source rejects cancel with its original read failure.
        if (error !== reason) throw error;
      })
      .finally(() => reader.releaseLock());
    return cancellation;
  }
  function abort() {
    const error = new StorageError("aborted", "Upload cancelled");
    controller.error(error);
    void cancel(error).catch(() => {});
  }
  const body = new ReadableStream<Uint8Array>(
    {
      start(value) {
        controller = value;
        input.signal?.addEventListener("abort", abort, { once: true });
        if (input.signal?.aborted) abort();
      },
      async pull(value) {
        try {
          const chunk = await reader.read();
          if (ended) return;
          if (chunk.done) {
            if (input.size !== undefined && bytes !== input.size) {
              throw new StorageError("invalid-input", "Upload size differs from declared size");
            }
            ended = true;
            detach();
            reader.releaseLock();
            value.close();
            return;
          }
          bytes += chunk.value.byteLength;
          if (input.maxBytes !== undefined && bytes > input.maxBytes) {
            throw new StorageError("too-large", "Upload exceeds the size limit");
          }
          if (input.size !== undefined && bytes > input.size) {
            throw new StorageError("invalid-input", "Upload exceeds its declared size");
          }
          value.enqueue(chunk.value);
        } catch (error) {
          value.error(error);
          await cancel(error).catch(() => {});
        }
      },
      cancel,
    },
    { highWaterMark: 0 },
  );
  return { body, size: () => bytes, cancel };
}
