import { startApp } from "lenso";
import { definePlugin, type Plugin } from "lenso/plugin";

/** The subset used by this adapter; Cloudflare's generated ExecutionContext is compatible. */
export interface WorkerExecutionContext {
  waitUntil(promise: Promise<unknown>): void;
}

export interface WorkerRequestContext {
  readonly request: Request;
  readonly executionContext: WorkerExecutionContext;
}

export interface WorkerApp {
  readonly plugins: readonly Plugin<unknown>[];
  readonly web: Plugin<{ fetch(request: Request): Promise<Response> }>;
}

/** Bindings belong to the platform: injecting them does not acquire or close them. */
export function createBindingsPlugin<Bindings>(options: {
  id: string;
  bindings: Bindings;
}): Plugin<Bindings> {
  return definePlugin({ id: options.id, setup: () => options.bindings });
}

function retainUntilBodyEnds(response: Response, stop: () => Promise<void>): Response {
  if (!response.body) return response;
  const reader = response.body.getReader();
  let cancelled = false;
  let stopping: Promise<void> | undefined;
  const finish = () => (stopping ??= stop().finally(() => reader.releaseLock()));
  const body = new ReadableStream<Uint8Array>(
    {
      async pull(controller) {
        try {
          const chunk = await reader.read();
          if (cancelled) return;
          if (chunk.done) {
            await finish();
            controller.close();
          } else {
            controller.enqueue(chunk.value);
          }
        } catch (error) {
          try {
            await finish();
          } catch (cleanupError) {
            controller.error(
              new AggregateError([error, cleanupError], "Worker stream cleanup failed"),
            );
            return;
          }
          controller.error(error);
        }
      },
      async cancel(reason) {
        cancelled = true;
        try {
          await reader.cancel(reason);
        } finally {
          await finish();
        }
      },
    },
    { highWaterMark: 0 },
  );
  return new Response(body, {
    status: response.status,
    statusText: response.statusText,
    headers: response.headers,
  });
}

/** A separate Fetch entrypoint. Assembly and startup run inside each incoming request. */
export function createWorkerHandler<Bindings>(
  assemble: (bindings: Bindings, context: WorkerRequestContext) => WorkerApp | Promise<WorkerApp>,
): {
  fetch(
    request: Request,
    bindings: Bindings,
    executionContext: WorkerExecutionContext,
  ): Promise<Response>;
} {
  return {
    async fetch(request, bindings, executionContext) {
      const definition = await assemble(bindings, { request, executionContext });
      const app = await startApp(definition);
      const stop = () => {
        request.signal.removeEventListener("abort", disconnect);
        return app.stop();
      };
      // Keep async finalizers alive after a client disconnects from the Worker event.
      const disconnect = () => executionContext.waitUntil(stop());
      request.signal.addEventListener("abort", disconnect, { once: true });
      try {
        if (request.signal.aborted) {
          await stop();
          request.signal.throwIfAborted();
        }
        const response = await app.get(definition.web).fetch(request);
        if (!response.body) {
          await stop();
          return response;
        }
        return retainUntilBodyEnds(response, stop);
      } catch (error) {
        try {
          await stop();
        } catch (cleanupError) {
          throw new AggregateError([error, cleanupError], "Worker request cleanup failed");
        }
        throw error;
      }
    },
  };
}
