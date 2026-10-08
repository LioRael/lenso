import { definePlugin, type Plugin } from "@lenso/core";
import type { WebService } from "./index";

export interface BunListenerOptions {
  id?: string;
  web: Plugin<WebService>;
  hostname: string;
  port: number;
  ingress(request: Request, url: URL): Response | undefined | Promise<Response | undefined>;
}

export interface BunListenerService {
  url: URL;
  port: number;
}

/** Owns a Bun listener and delegates accepted requests to one declared Web instance. */
export function createBunListenerPlugin(options: BunListenerOptions): Plugin<BunListenerService> {
  return definePlugin({
    id: options.id ?? "bun-listener",
    requires: [options.web],
    setup(context) {
      const web = context.get(options.web);
      const server = Bun.serve({
        hostname: options.hostname,
        port: options.port,
        fetch: async (request) => {
          const response = await options.ingress(request, server.url);
          return response ?? web.fetch(request);
        },
      });
      context.onCleanup(() => server.stop(true));
      if (server.port === undefined) throw new Error("Bun TCP listener did not report a port");
      return { url: server.url, port: server.port };
    },
  });
}
