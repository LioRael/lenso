import { createORPCClient } from "@orpc/client";
import { RPCLink } from "@orpc/client/fetch";
import type { AnyRouter, RouterClient } from "@orpc/server";

export interface ClientOptions {
  fetch?: (input: RequestInfo | URL, init?: RequestInit) => Promise<Response>;
  headers?: HeadersInit;
}

/** Import the router with `import type` so server code cannot enter the browser. */
export function createClient<R extends AnyRouter>(
  url: string | URL,
  options: ClientOptions = {},
): RouterClient<R> {
  const endpoint = new URL(url, globalThis.location?.href);
  return createORPCClient(
    new RPCLink({
      origin: endpoint.origin,
      url: `${endpoint.pathname}${endpoint.search}${endpoint.hash}` as `/${string}`,
      fetch: options.fetch,
      headers: options.headers ? new Headers(options.headers) : undefined,
    }),
  );
}
