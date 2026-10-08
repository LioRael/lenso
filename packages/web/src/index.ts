import { RPCHandler } from '@orpc/server/fetch';
import type { Router } from '@orpc/server';
import { definePlugin, type Plugin, type PluginContext } from 'lenso';

/** Request context stays in the optional Web package, outside the core SDK. */
export interface WebContext { request: Request }
export interface WebService { fetch(request: Request): Promise<Response> }

export interface WebPluginOptions<R extends Router<any, WebContext>> {
  id?: string;
  requires: readonly Plugin<unknown>[];
  router(context: PluginContext): R;
  prefix?: `/${string}`;
}

/** The app owns its listener. This adapter only handles Fetch requests. */
export function createWebPlugin<R extends Router<any, WebContext>>(options: WebPluginOptions<R>): Plugin<WebService> {
  const prefix = options.prefix ?? '/rpc';
  if (!prefix.startsWith('/') || prefix.endsWith('/')) {
    throw new Error('Web RPC prefix must start with / and have no trailing /');
  }
  return definePlugin({
    id: options.id ?? 'web',
    requires: options.requires,
    setup(context) {
      const handler = new RPCHandler<WebContext>(options.router(context));
      return {
        async fetch(request) {
          const result = await handler.handle(request, { prefix, context: { request } });
          return result.matched ? result.response : new Response('Not found', { status: 404 });
        },
      };
    },
  });
}
