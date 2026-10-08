/** Resources belong to the body/producer lifetime, not the Response promise. */
export interface FetchContext {
  request: Request;
  signal: AbortSignal;
  onCleanup(cleanup: () => void | Promise<void>): void;
  /** Register work that may outlive the response, especially non-cancellable producers. */
  waitUntil(work: Promise<unknown>): void;
}

export interface FetchOptions {
  /** Covers headers and body. Omitted means no application deadline. */
  timeoutMs?: number;
  /** Reject larger source chunks; the adapter itself never prefetches. Default 64 KiB. */
  maxChunkBytes?: number;
  /** Sanitized diagnostics only: no provider errors, headers or credentials. */
  onError?: (phase: "handler" | "body" | "work" | "cleanup") => void;
}

export type FetchHandler = (context: FetchContext) => Response | Promise<Response>;

export function createRequestTask(request: Request, handler: FetchHandler, options: FetchOptions) {
  const abort = new AbortController();
  const cleanups: (() => void | Promise<void>)[] = [];
  const work = new Set<Promise<void>>();
  let finished = false;
  let finishing: Promise<void> | undefined;
  let cancelBody: (() => Promise<void>) | undefined;
  let timeout = false;
  let failure = false;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let complete!: () => void;
  const completed = new Promise<void>((resolve) => {
    complete = resolve;
  });
  const report = (phase: Parameters<NonNullable<FetchOptions["onError"]>>[0]) => {
    try {
      options.onError?.(phase);
    } catch {
      /* Diagnostics cannot break finalization. */
    }
  };
  const finish = () =>
    (finishing ??= (async () => {
      // New work/cleanup can still be registered by a settling producer.
      while (work.size) await Promise.all(work);
      if (timer) clearTimeout(timer);
      request.signal.removeEventListener("abort", disconnect);
      abort.signal.removeEventListener("abort", cancelled);
      finished = true;
      for (const cleanup of cleanups.reverse()) {
        try {
          await cleanup();
        } catch {
          report("cleanup");
        }
      }
      complete();
    })());
  const disconnect = () => abort.abort(request.signal.reason);
  const cancelled = () => {
    void cancelBody?.();
  };
  request.signal.addEventListener("abort", disconnect, { once: true });
  abort.signal.addEventListener("abort", cancelled, { once: true });
  if (request.signal.aborted) disconnect();
  if (options.timeoutMs !== undefined)
    timer = setTimeout(() => {
      timeout = true;
      abort.abort(new DOMException("Request deadline exceeded", "TimeoutError"));
    }, options.timeoutMs);

  const context: FetchContext = {
    request: new Request(request, { signal: abort.signal }),
    signal: abort.signal,
    onCleanup(cleanup) {
      if (finished) throw new Error("Request lifetime has ended");
      cleanups.push(cleanup);
    },
    waitUntil(promise) {
      if (finished) throw new Error("Request lifetime has ended");
      const tracked = Promise.resolve(promise)
        .then(
          () => {},
          () => {
            report("work");
            failure = true;
            abort.abort(new Error("Request work failed"));
          },
        )
        .finally(() => work.delete(tracked));
      work.add(tracked);
    },
  };

  const handled = (async () => {
    try {
      abort.signal.throwIfAborted();
      const response = await handler(context);
      if (!response.body) {
        await finish();
        return response;
      }
      const reader = response.body.getReader();
      let streamController: ReadableStreamDefaultController<Uint8Array>;
      let closing: Promise<void> | undefined;
      let reading: ReturnType<typeof reader.read> | undefined;
      const end = (cancel: boolean) =>
        (closing ??= (async () => {
          try {
            if (cancel) await reader.cancel(abort.signal.reason);
            await reading;
          } catch {
            report("body");
          } finally {
            reader.releaseLock();
            await finish();
          }
        })());
      cancelBody = () => {
        if (!closing) {
          streamController.error(new DOMException("Response cancelled", "AbortError"));
        }
        return end(true);
      };
      const body = new ReadableStream<Uint8Array>(
        {
          start(controller) {
            streamController = controller;
          },
          async pull(controller) {
            try {
              const read = reader.read();
              reading = read;
              const chunk = await read;
              if (closing) return;
              if (chunk.done) {
                await end(false);
                controller.close();
              } else {
                if (
                  !(chunk.value instanceof Uint8Array) ||
                  chunk.value.byteLength > (options.maxChunkBytes ?? 65536)
                ) {
                  throw new Error("Response chunk exceeds byte limit");
                }
                controller.enqueue(chunk.value);
              }
            } catch {
              if (closing) return;
              report("body");
              controller.error(new Error("Response body failed"));
              abort.abort(new Error("Response body failed"));
              await end(true);
            }
          },
          cancel() {
            // Consumer cancellation already closed the wrapper; don't error it again.
            const ending = end(true);
            abort.abort(new DOMException("Response cancelled", "AbortError"));
            return ending;
          },
        },
        { highWaterMark: 0 },
      );
      if (abort.signal.aborted) {
        await cancelBody();
        abort.signal.throwIfAborted();
      }
      return new Response(body, {
        status: response.status,
        statusText: response.statusText,
        headers: response.headers,
      });
    } catch {
      if (!abort.signal.aborted) {
        report("handler");
        failure = true;
        abort.abort(new Error("Request handler failed"));
      }
      await finish();
      return new Response("Internal server error", { status: 500 });
    }
  })();

  // Deadline/disconnect returns promptly; the late handler/body remains owned above.
  let removeAbort: (() => void) | undefined;
  const interrupted = new Promise<Response>((resolve) => {
    const listener = () =>
      resolve(
        new Response(
          failure ? "Internal server error" : timeout ? "Request timeout" : "Request cancelled",
          { status: failure ? 500 : timeout ? 504 : 499 },
        ),
      );
    if (abort.signal.aborted) listener();
    else {
      abort.signal.addEventListener("abort", listener, { once: true });
      removeAbort = () => abort.signal.removeEventListener("abort", listener);
    }
  });
  return {
    response: Promise.race([handled, interrupted]).finally(() => removeAbort?.()),
    completed,
    abort: () => abort.abort(new DOMException("Web service stopped", "AbortError")),
  };
}
