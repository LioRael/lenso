import { once } from "node:events";
import { createServer } from "node:http";

const titleCase = (value) =>
  value
    .split(/[_-]+/u)
    .filter(Boolean)
    .map((part) => `${part[0]?.toUpperCase() ?? ""}${part.slice(1)}`)
    .join(" ");

const field = (name, fieldType, options) => ({
  field_type: fieldType,
  label: options.label ?? titleCase(name),
  name,
  nullable: options.nullable ?? false,
});

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

const handleAdminDataRequest = async ({ basePath, data, requestUrl }) => {
  const url = new URL(requestUrl, "http://127.0.0.1");
  const prefix = `${basePath}/admin/`;
  if (!url.pathname.startsWith(prefix)) {
    return null;
  }
  const parts = url.pathname.slice(prefix.length).split("/").filter(Boolean);
  const [entity, id] = parts;
  if (!entity || parts.length > 2) {
    return {
      body: {
        error: { code: "not_found", message: "admin endpoint not found" },
      },
      statusCode: 404,
    };
  }
  const source = data[entity];
  if (!source) {
    return {
      body: {
        error: { code: "not_found", message: `${entity} admin data not found` },
      },
      statusCode: 404,
    };
  }
  if (id) {
    const record = await source.detail(decodeURIComponent(id));
    return {
      body: { record: record ?? null },
      statusCode: record ? 200 : 404,
    };
  }
  const limit = Number(url.searchParams.get("limit") ?? "50");
  const cursor = url.searchParams.get("cursor") ?? undefined;
  const page = await source.list({
    limit: Number.isFinite(limit) ? limit : 50,
    ...(cursor ? { cursor } : {}),
  });
  return {
    body: page,
    statusCode: 200,
  };
};

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

export const textField = (name, options = {}) =>
  field(name, { kind: "string" }, options);

export const integerField = (name, options = {}) =>
  field(name, { kind: "integer" }, options);

export const booleanField = (name, options = {}) =>
  field(name, { kind: "boolean" }, options);

export const timestampField = (name, options = {}) =>
  field(name, { kind: "timestamp" }, options);

export const jsonField = (name, options = {}) =>
  field(name, { kind: "json" }, options);

export const defineSchemaEntity = ({
  fields,
  label,
  name,
  readCapability,
}) => ({
  fields,
  label,
  name,
  read_capability: readCapability,
});

export const schemaAdmin = (entities) => ({
  entities,
  kind: "schema",
});

export const serveRemoteModule = async (manifest, options = {}) => {
  const host = options.host ?? "127.0.0.1";
  const port = options.port ?? 4100;
  const basePath = normalizeBasePath(options.basePath ?? "/lenso/module/v1");
  const manifestPath = `${basePath}/manifest`;

  const server = createServer(async (request, response) => {
    if (request.method === "GET" && request.url === manifestPath) {
      sendJson(response, 200, manifest);
      return;
    }
    if (request.method === "GET") {
      const adminResult = await handleAdminDataRequest({
        basePath,
        data: options.data ?? {},
        requestUrl: request.url ?? "",
      });
      if (adminResult) {
        sendJson(response, adminResult.statusCode, adminResult.body);
        return;
      }
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
