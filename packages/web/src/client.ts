import { createORPCClient } from '@orpc/client';
import { RPCLink } from '@orpc/client/fetch';
import type { AnyRouter, RouterClient } from '@orpc/server';

export interface ClientOptions {
  fetch?: typeof globalThis.fetch;
  headers?: HeadersInit;
}

/** Import the router with `import type` so server code cannot enter the browser. */
export function createClient<R extends AnyRouter>(url: string | URL, options: ClientOptions = {}): RouterClient<R> {
  return createORPCClient(new RPCLink({ url, fetch: options.fetch, headers: options.headers ? new Headers(options.headers) : undefined }));
}
