import { once } from "node:events";
import { createServer } from "node:http";

export const defineRemoteModule = (definition) => {
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

export const serveRemoteModule = async (manifest, options = {}) => {
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
  };

  options.onReady?.(served);
  return served;
};

const normalizeBasePath = (basePath) => {
  const trimmed = basePath.replace(/\/+$/u, "");
  if (!trimmed.startsWith("/")) {
    return `/${trimmed}`;
  }
  return trimmed || "/lenso/module/v1";
};

const sendJson = (response, statusCode, body) => {
  response.writeHead(statusCode, {
    "content-type": "application/json; charset=utf-8",
  });
  response.end(JSON.stringify(body));
};
