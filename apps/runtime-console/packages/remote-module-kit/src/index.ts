import { once } from "node:events";
import { createServer } from "node:http";
import type { Server, ServerResponse } from "node:http";

export interface RemoteModuleConsoleSurface {
  name: string;
  label: string;
  area: "runtime" | "operations" | "data" | "configuration" | string;
  route: string;
  package: {
    name: string;
    export: string;
  };
  required_capabilities?: readonly string[];
  icon?: string;
  navigation?: {
    workspace?: {
      id: string;
      label: string;
      icon?: string;
    };
    group?: {
      id: string;
      label: string;
      order?: number;
    } | null;
    order?: number;
  };
}

export interface RemoteModuleManifest {
  name: string;
  version: string;
  source: "remote";
  capabilities: readonly string[];
  http_routes: readonly unknown[];
  runtime: {
    functions: readonly unknown[];
  };
  admin: unknown | null;
  console?: readonly RemoteModuleConsoleSurface[];
}

export interface RemoteModuleDefinition {
  name: string;
  version?: string;
  capabilities?: readonly string[];
  httpRoutes?: readonly unknown[];
  runtimeFunctions?: readonly unknown[];
  admin?: unknown | null;
  console?: readonly RemoteModuleConsoleSurface[];
}

export interface ServedRemoteModule {
  baseUrl: string;
  manifestUrl: string;
  server: Server;
  close: () => Promise<void>;
}

export interface ServeRemoteModuleOptions {
  host?: string;
  port?: number;
  basePath?: string;
  onReady?: (server: ServedRemoteModule) => void;
}

const normalizeBasePath = (basePath: string) => {
  const trimmed = basePath.replace(/\/+$/u, "");
  if (!trimmed.startsWith("/")) {
    return `/${trimmed}`;
  }
  return trimmed || "/lenso/module/v1";
};

const sendJson = (
  response: ServerResponse,
  statusCode: number,
  body: unknown
) => {
  response.writeHead(statusCode, {
    "content-type": "application/json; charset=utf-8",
  });
  response.end(JSON.stringify(body));
};

export const defineRemoteModule = (
  definition: RemoteModuleDefinition
): RemoteModuleManifest => {
  if (!definition.name.trim()) {
    throw new Error("Remote module name is required");
  }
  return {
    admin: definition.admin ?? null,
    capabilities: definition.capabilities ?? [],
    console: definition.console ?? [],
    http_routes: definition.httpRoutes ?? [],
    name: definition.name,
    runtime: {
      functions: definition.runtimeFunctions ?? [],
    },
    source: "remote",
    version: definition.version ?? "0.1.0",
  };
};

export const serveRemoteModule = async (
  manifest: RemoteModuleManifest,
  options: ServeRemoteModuleOptions = {}
): Promise<ServedRemoteModule> => {
  const host = options.host ?? "127.0.0.1";
  const port = options.port ?? 4100;
  const basePath = normalizeBasePath(options.basePath ?? "/lenso/module/v1");
  const manifestPath = `${basePath}/manifest`;

  const server = createServer((request, response) => {
    if (request.method === "GET" && request.url === manifestPath) {
      sendJson(response, 200, manifest);
      return;
    }

    sendJson(response, 404, {
      error: {
        code: "not_found",
        message: `${manifest.name} remote module endpoint not found`,
      },
    });
  });

  server.listen(port, host);
  await once(server, "listening");

  const address = server.address();
  const boundPort =
    typeof address === "object" && address ? address.port : port;
  const baseUrl = `http://${host}:${boundPort}${basePath}`;
  const served = {
    baseUrl,
    close: async () => {
      server.close();
      await once(server, "close");
    },
    manifestUrl: `${baseUrl}/manifest`,
    server,
  } satisfies ServedRemoteModule;

  options.onReady?.(served);
  return served;
};
