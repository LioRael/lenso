import { StorageError, validateContentType, type PutObjectInput } from "./index";

export function validateUpload(input: PutObjectInput): void {
  if (input.contentType !== undefined) validateContentType(input.contentType);
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
