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

const route = (method, path, options = {}) => ({
  ...(options.capability ? { capability: options.capability } : {}),
  ...(options.displayName ? { display_name: options.displayName } : {}),
  method,
  path,
  ...(options.storyTitle ? { story_title: options.storyTitle } : {}),
});

const routeKey = (method, path) => `${method} ${path}`;

const matchRoutePath = (pattern, pathname) => {
  const patternParts = pattern.split("/").filter(Boolean);
  const pathParts = pathname.split("/").filter(Boolean);
  if (patternParts.length !== pathParts.length) {
    return null;
  }
  const params = {};
  for (const [index, patternPart] of patternParts.entries()) {
    const pathPart = pathParts[index];
    if (!pathPart) {
      return null;
    }
    if (patternPart.startsWith("{") && patternPart.endsWith("}")) {
      const paramName = patternPart.slice(1, -1);
      if (!paramName) {
        return null;
      }
      params[paramName] = decodeURIComponent(pathPart);
      continue;
    }
    if (patternPart !== pathPart) {
      return null;
    }
  }
  return params;
};

const readBody = async (request) => {
  const chunks = [];
  for await (const chunk of request) {
    chunks.push(Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk));
  }
  if (chunks.length === 0) {
    return;
  }
  const text = Buffer.concat(chunks).toString("utf-8");
  if (!text.trim()) {
    return;
  }
  try {
    return JSON.parse(text);
  } catch {
    return text;
  }
};

const normalizeHandlerResult = (result) => {
  if (
    typeof result === "object" &&
    result !== null &&
    "body" in result &&
    ("statusCode" in result || Object.keys(result).length <= 2)
  ) {
    return {
      body: result.body,
      statusCode: result.statusCode ?? 200,
    };
  }
  return { body: result ?? null, statusCode: 200 };
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

const handleHttpRouteRequest = async ({
  basePath,
  handlers,
  manifest,
  request,
}) => {
  const { method } = request;
  if (!method) {
    return null;
  }
  const url = new URL(request.url ?? "", "http://127.0.0.1");
  if (!url.pathname.startsWith(`${basePath}/`)) {
    return null;
  }
  const modulePath = url.pathname.slice(basePath.length) || "/";
  for (const declaredRoute of manifest.http_routes) {
    if (declaredRoute.method !== method) {
      continue;
    }
    const params = matchRoutePath(declaredRoute.path, modulePath);
    if (!params) {
      continue;
    }
    const handler =
      handlers[routeKey(declaredRoute.method, declaredRoute.path)];
    if (!handler) {
      return {
        body: {
          error: {
            code: "not_found",
            message: `${declaredRoute.method} ${declaredRoute.path} handler not found`,
          },
        },
        statusCode: 404,
      };
    }
    const body = await readBody(request);
    return normalizeHandlerResult(
      await handler({
        body,
        params,
        request,
        url,
      })
    );
  }
  return null;
};

const runtimeFunctionQueue = (name) => name.split(".")[0] ?? name;

const handleRuntimeFunctionRequest = async ({
  basePath,
  handlers,
  request,
}) => {
  if (request.method !== "POST") {
    return null;
  }
  const url = new URL(request.url ?? "", "http://127.0.0.1");
  const prefix = `${basePath}/runtime/functions/`;
  if (!(url.pathname.startsWith(prefix) && url.pathname.endsWith("/invoke"))) {
    return null;
  }
  const functionName = decodeURIComponent(
    url.pathname.slice(prefix.length, -"/invoke".length)
  );
  if (!functionName || functionName.includes("/")) {
    return {
      body: {
        error: {
          code: "not_found",
          message: "runtime function endpoint not found",
        },
      },
      statusCode: 404,
    };
  }
  const handler = handlers[functionName];
  if (!handler) {
    return {
      body: {
        error: {
          code: "not_found",
          message: `${functionName} runtime function handler not found`,
        },
      },
      statusCode: 404,
    };
  }
  const invocation = await readBody(request);
  const output = await handler({
    input: invocation?.input,
    invocation,
    request,
  });
  return {
    body: { output: output ?? null },
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
    ...(definition.lifecycle ? { lifecycle: definition.lifecycle } : {}),
    name: definition.name,
    runtime: {
      functions: definition.runtimeFunctions ?? [],
    },
    source: "remote",
    version: definition.version ?? "0.1.0",
  };
};

export const getRoute = (path, options = {}) => route("GET", path, options);

export const postRoute = (path, options = {}) => route("POST", path, options);

export const putRoute = (path, options = {}) => route("PUT", path, options);

export const patchRoute = (path, options = {}) => route("PATCH", path, options);

export const deleteRoute = (path, options = {}) =>
  route("DELETE", path, options);

export const runtimeFunction = (name, options = {}) => ({
  ...(options.inputSchema ? { input_schema: options.inputSchema } : {}),
  queue: options.queue ?? runtimeFunctionQueue(name),
  ...(options.retryPolicy ? { retry_policy: options.retryPolicy } : {}),
  name,
  version: options.version ?? 1,
});

export const everyStartup = (name, functionName, options = {}) => ({
  function_name: functionName,
  input: options.input ?? {},
  name,
  required: options.required ?? true,
  run_policy: "every_startup",
});

export const lifecycle = ({ activationJobs, startupChecks }) => ({
  activation_jobs: activationJobs ?? [],
  startup_checks: startupChecks ?? [],
});

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
    const runtimeResult = await handleRuntimeFunctionRequest({
      basePath,
      handlers: options.runtime ?? {},
      request,
    });
    if (runtimeResult) {
      sendJson(response, runtimeResult.statusCode, runtimeResult.body);
      return;
    }
    const httpResult = await handleHttpRouteRequest({
      basePath,
      handlers: options.http ?? {},
      manifest,
      request,
    });
    if (httpResult) {
      sendJson(response, httpResult.statusCode, httpResult.body);
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
