import { RPCHandler } from "@orpc/server/fetch";
import type { Router } from "@orpc/server";
import { definePlugin, type Plugin, type PluginContext } from "@lenso/core";
import { createRequestTask, type FetchContext, type FetchOptions } from "./lifetime";
export type { FetchContext, FetchHandler, FetchOptions } from "./lifetime";

/** Request context stays in the optional Web package, outside the core SDK. */
export interface WebContext extends FetchContext {}
export interface WebService {
  fetch(request: Request): Promise<Response>;
}

export interface WebPluginOptions<R extends Router<any, WebContext>> extends FetchOptions {
  id?: string;
  requires: readonly Plugin<unknown>[];
  router(context: PluginContext): R;
  prefix?: `/${string}`;
  /** Raw bytes first; undefined falls through to the standard oRPC handler. */
  fetch?: (
    context: PluginContext,
  ) => (context: WebContext) => Response | undefined | Promise<Response | undefined>;
}

/** The app owns its listener. This adapter only handles Fetch requests. */
export function createWebPlugin<R extends Router<any, WebContext>>(
  options: WebPluginOptions<R>,
): Plugin<WebService> {
  const prefix = options.prefix ?? "/rpc";
  if (!prefix.startsWith("/") || prefix.endsWith("/")) {
    throw new Error("Web RPC prefix must start with / and have no trailing /");
  }
  if (
    options.timeoutMs !== undefined &&
    (!Number.isFinite(options.timeoutMs) || options.timeoutMs <= 0)
  ) {
    throw new Error("Web timeoutMs must be positive and finite");
  }
  if (
    options.maxChunkBytes !== undefined &&
    (!Number.isSafeInteger(options.maxChunkBytes) || options.maxChunkBytes <= 0)
  ) {
    throw new Error("Web maxChunkBytes must be a positive safe integer");
  }
  return definePlugin({
    id: options.id ?? "web",
    requires: options.requires,
    setup(context) {
      const handler = new RPCHandler<WebContext>(options.router(context));
      const raw = options.fetch?.(context);
      const active = new Set<ReturnType<typeof createRequestTask>>();
      let stopped = false;
      context.onCleanup(async () => {
        stopped = true;
        for (const task of active) task.abort();
        await Promise.all([...active].map((task) => task.completed));
      });
      return {
        async fetch(request) {
          if (stopped) return new Response("Web service stopped", { status: 503 });
          const task = createRequestTask(
            request,
            async (webContext) => {
              const response = await raw?.(webContext);
              if (response) return response;
              webContext.signal.throwIfAborted();
              const result = await handler.handle(webContext.request, {
                prefix,
                context: webContext,
              });
              return result.matched ? result.response : new Response("Not found", { status: 404 });
            },
            options,
          );
          active.add(task);
          void task.completed.then(() => active.delete(task));
          return task.response;
        },
      };
    },
  });
}
