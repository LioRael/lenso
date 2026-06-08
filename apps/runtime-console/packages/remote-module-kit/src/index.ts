import { once } from "node:events";
import { createServer } from "node:http";
import type { IncomingMessage, Server, ServerResponse } from "node:http";

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
  http_routes: readonly RemoteHttpRoute[];
  runtime: {
    functions: readonly unknown[];
  };
  admin: unknown | null;
  console?: readonly RemoteModuleConsoleSurface[];
}

export type RemoteHttpMethod = "GET" | "POST" | "PUT" | "PATCH" | "DELETE";

export interface RemoteHttpRoute {
  method: RemoteHttpMethod;
  path: string;
  capability?: string;
  display_name?: string;
  story_title?: string;
}

export interface RemoteHttpRouteOptions {
  capability?: string;
  displayName?: string;
  storyTitle?: string;
}

export interface RemoteHttpHandlerContext {
  body: unknown;
  params: Record<string, string>;
  request: IncomingMessage;
  url: URL;
}

export type RemoteHttpHandlerResult =
  | unknown
  | {
      body: unknown;
      statusCode?: number;
    };

export type RemoteHttpHandler = (
  context: RemoteHttpHandlerContext
) => RemoteHttpHandlerResult | Promise<RemoteHttpHandlerResult>;

export type SchemaFieldType =
  | { kind: "string" }
  | { kind: "integer" }
  | { kind: "boolean" }
  | { kind: "timestamp" }
  | { kind: "json" };

export interface SchemaField {
  name: string;
  label: string;
  field_type: SchemaFieldType;
  nullable: boolean;
}

export interface SchemaEntity {
  name: string;
  label: string;
  fields: readonly SchemaField[];
  read_capability: string;
}

export interface SchemaAdminSurface {
  kind: "schema";
  entities: readonly SchemaEntity[];
}

export interface RemoteModuleDefinition {
  name: string;
  version?: string;
  capabilities?: readonly string[];
  httpRoutes?: readonly RemoteHttpRoute[];
  runtimeFunctions?: readonly unknown[];
  admin?: unknown | null;
  console?: readonly RemoteModuleConsoleSurface[];
}

export interface RemoteAdminPage {
  records: readonly unknown[];
  next_cursor?: string | null;
}

export interface RemoteAdminDataSource {
  list: (query: {
    limit: number;
    cursor?: string;
  }) => RemoteAdminPage | Promise<RemoteAdminPage>;
  detail: (
    id: string
  ) => unknown | null | undefined | Promise<unknown | null | undefined>;
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
  data?: Record<string, RemoteAdminDataSource>;
  http?: Record<string, RemoteHttpHandler>;
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

const route = (
  method: RemoteHttpMethod,
  path: string,
  options: RemoteHttpRouteOptions = {}
): RemoteHttpRoute => ({
  ...(options.capability ? { capability: options.capability } : {}),
  ...(options.displayName ? { display_name: options.displayName } : {}),
  method,
  path,
  ...(options.storyTitle ? { story_title: options.storyTitle } : {}),
});

const routeKey = (method: RemoteHttpMethod, path: string) =>
  `${method} ${path}`;

const matchRoutePath = (
  pattern: string,
  pathname: string
): Record<string, string> | null => {
  const patternParts = pattern.split("/").filter(Boolean);
  const pathParts = pathname.split("/").filter(Boolean);
  if (patternParts.length !== pathParts.length) {
    return null;
  }
  const params: Record<string, string> = {};
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

const readBody = async (request: IncomingMessage): Promise<unknown> => {
  const chunks: Buffer[] = [];
  for await (const chunk of request) {
    chunks.push(Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk));
  }
  if (chunks.length === 0) {
    return undefined;
  }
  const text = Buffer.concat(chunks).toString("utf-8");
  if (!text.trim()) {
    return undefined;
  }
  try {
    return JSON.parse(text);
  } catch {
    return text;
  }
};

const normalizeHandlerResult = (
  result: RemoteHttpHandlerResult
): { body: unknown; statusCode: number } => {
  if (
    typeof result === "object" &&
    result !== null &&
    "body" in result &&
    ("statusCode" in result || Object.keys(result).length <= 2)
  ) {
    const response = result as { body: unknown; statusCode?: number };
    return {
      body: response.body,
      statusCode: response.statusCode ?? 200,
    };
  }
  return { body: result ?? null, statusCode: 200 };
};

const handleHttpRouteRequest = async ({
  basePath,
  handlers,
  manifest,
  request,
}: {
  basePath: string;
  handlers: Record<string, RemoteHttpHandler>;
  manifest: RemoteModuleManifest;
  request: IncomingMessage;
}): Promise<{ body: unknown; statusCode: number } | null> => {
  const method = request.method as RemoteHttpMethod | undefined;
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

interface FieldOptions {
  label?: string;
  nullable?: boolean;
}

const titleCase = (value: string) =>
  value
    .split(/[_-]+/u)
    .filter(Boolean)
    .map((part) => `${part[0]?.toUpperCase() ?? ""}${part.slice(1)}`)
    .join(" ");

const field = (
  name: string,
  fieldType: SchemaFieldType,
  options: FieldOptions
): SchemaField => ({
  field_type: fieldType,
  label: options.label ?? titleCase(name),
  name,
  nullable: options.nullable ?? false,
});

const handleAdminDataRequest = async ({
  basePath,
  data,
  requestUrl,
}: {
  basePath: string;
  data: Record<string, RemoteAdminDataSource>;
  requestUrl: string;
}): Promise<{ body: unknown; statusCode: number } | null> => {
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

export const getRoute = (path: string, options: RemoteHttpRouteOptions = {}) =>
  route("GET", path, options);

export const postRoute = (path: string, options: RemoteHttpRouteOptions = {}) =>
  route("POST", path, options);

export const putRoute = (path: string, options: RemoteHttpRouteOptions = {}) =>
  route("PUT", path, options);

export const patchRoute = (
  path: string,
  options: RemoteHttpRouteOptions = {}
) => route("PATCH", path, options);

export const deleteRoute = (
  path: string,
  options: RemoteHttpRouteOptions = {}
) => route("DELETE", path, options);

export const textField = (name: string, options: FieldOptions = {}) =>
  field(name, { kind: "string" }, options);

export const integerField = (name: string, options: FieldOptions = {}) =>
  field(name, { kind: "integer" }, options);

export const booleanField = (name: string, options: FieldOptions = {}) =>
  field(name, { kind: "boolean" }, options);

export const timestampField = (name: string, options: FieldOptions = {}) =>
  field(name, { kind: "timestamp" }, options);

export const jsonField = (name: string, options: FieldOptions = {}) =>
  field(name, { kind: "json" }, options);

export const defineSchemaEntity = ({
  fields,
  label,
  name,
  readCapability,
}: {
  name: string;
  label: string;
  fields: readonly SchemaField[];
  readCapability: string;
}): SchemaEntity => ({
  fields,
  label,
  name,
  read_capability: readCapability,
});

export const schemaAdmin = (
  entities: readonly SchemaEntity[]
): SchemaAdminSurface => ({
  entities,
  kind: "schema",
});

export const serveRemoteModule = async (
  manifest: RemoteModuleManifest,
  options: ServeRemoteModuleOptions = {}
): Promise<ServedRemoteModule> => {
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
  } satisfies ServedRemoteModule;

  options.onReady?.(served);
  return served;
};
