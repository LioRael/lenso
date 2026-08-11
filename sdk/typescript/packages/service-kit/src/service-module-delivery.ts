/* eslint-disable complexity, func-style, no-use-before-define */
import { timingSafeEqual } from "node:crypto";
import { once } from "node:events";
import { createServer } from "node:http";
import type {
  IncomingMessage,
  Server as HttpServer,
  ServerResponse,
} from "node:http";
import { createServer as createHttp2Server } from "node:http2";
import type { Http2Server, ServerHttp2Stream } from "node:http2";

export interface ModuleConsoleSurface {
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

export interface ProviderModuleManifest {
  name: string;
  version: string;
  source: "service";
  compatibility?: ServiceModuleCompatibility;
  service?: ServiceModuleProviderMetadata;
  story_display: readonly ModuleStoryDisplayDescriptor[];
  capabilities: readonly string[];
  dependencies: readonly string[];
  http_routes: readonly ModuleHttpRoute[];
  runtime: {
    functions: readonly ModuleRuntimeFunctionDeclaration[];
  };
  events?: ModuleEventSurface;
  lifecycle?: ModuleLifecycleSurface;
  admin: unknown | null;
  console?: readonly ModuleConsoleSurface[];
}

export interface ServiceModuleCompatibility {
  console_package_api?: string;
  lenso?: {
    min_version?: string;
    max_version?: string;
  };
  required_host_features?: readonly string[];
}

export interface ServiceModuleDeploymentMetadata {
  target?: string;
  commands?: readonly string[];
  compose_service?: string;
}

export interface ServiceModuleProviderMetadata {
  deployment?: ServiceModuleDeploymentMetadata;
  name?: string;
  required_env?: readonly string[];
  status_path?: string;
  status_url?: string;
  transports?: readonly string[];
  version?: string;
}

export type ModuleProviderStatusState = "ready" | "degraded" | "starting";

export interface ModuleProviderStatusCheck {
  name: string;
  status: "ok" | "warning" | "error";
  detail?: string;
}

export interface ModuleProviderStatus {
  moduleName: string;
  serviceName: string;
  version: string;
  protocolVersion: string;
  transports: readonly string[];
  state: ModuleProviderStatusState;
  checks: readonly ModuleProviderStatusCheck[];
  manifestUrl: string;
}

export interface ModuleProviderStatusOptions {
  checks?:
    | readonly ModuleProviderStatusCheck[]
    | (() =>
        | readonly ModuleProviderStatusCheck[]
        | Promise<readonly ModuleProviderStatusCheck[]>);
  state?: ModuleProviderStatusState;
}

export type ServiceStatusState = ModuleProviderStatusState;

export type ServiceStatusCheck = ModuleProviderStatusCheck;

export interface ServiceModuleStatusSummary {
  name: string;
  version: string;
}

export interface ServiceStatus {
  serviceName: string;
  version: string;
  protocolVersion: string;
  transports: readonly string[];
  state: ServiceStatusState;
  checks: readonly ServiceStatusCheck[];
  manifestUrl: string;
  modules: readonly ServiceModuleStatusSummary[];
}

export interface ServiceStatusOptions {
  checks?:
    | readonly ServiceStatusCheck[]
    | (() =>
        | readonly ServiceStatusCheck[]
        | Promise<readonly ServiceStatusCheck[]>);
  state?: ServiceStatusState;
}

export type ModuleStoryDisplaySource =
  | {
      kind: "execution_name";
      name: string;
    }
  | {
      kind: "http_request";
      method: string;
      path: string;
    };

export interface ModuleStoryDisplayDescriptor {
  source: ModuleStoryDisplaySource;
  display_name: string;
  story_title?: string;
}

export type ModuleHttpMethod = "GET" | "POST" | "PUT" | "PATCH" | "DELETE";

export type ServiceOperationIdempotency =
  | "none"
  | "idempotent"
  | "requires_key";

export interface ServiceOperationSafeProbe {
  method?: ModuleHttpMethod | string;
  path?: string;
  input?: unknown;
  expectStatus?: number;
}

export interface ServiceOperationMetadata {
  operationId?: string;
  summary?: string;
  inputSchema?: unknown;
  outputSchema?: unknown;
  safeProbe?: ServiceOperationSafeProbe;
  timeoutMs?: number;
  idempotency?: ServiceOperationIdempotency;
}

export interface ModuleHttpRoute {
  method: ModuleHttpMethod;
  path: string;
  capability?: string;
  display_name?: string;
  operation?: ServiceOperationMetadata;
  story_title?: string;
}

export interface ModuleHttpRouteOptions {
  capability?: string;
  displayName?: string;
  operation?: ServiceOperationMetadata;
  storyTitle?: string;
}

export interface LensoInvocationContext {
  requestId?: string;
  correlationId?: string;
  causationId?: string;
  providerName?: string;
  moduleName?: string;
  operationId?: string;
  operationKind?: string;
  actorKind?: string;
  traceparent?: string;
}

const firstHeaderValue = (value: string | string[] | undefined) =>
  Array.isArray(value) ? value[0] : value;

export const readLensoInvocationContext = (
  request: IncomingMessage
): LensoInvocationContext => {
  const { headers } = request;
  const requestId = firstHeaderValue(headers["x-request-id"]);
  const correlationId = firstHeaderValue(headers["x-lenso-correlation-id"]);
  const causationId = firstHeaderValue(headers["x-lenso-causation-id"]);
  const providerName = firstHeaderValue(headers["x-lenso-provider"]);
  const moduleName = firstHeaderValue(headers["x-lenso-module"]);
  const operationId = firstHeaderValue(headers["x-lenso-operation"]);
  const operationKind = firstHeaderValue(headers["x-lenso-operation-kind"]);
  const actorKind = firstHeaderValue(headers["x-lenso-actor-kind"]);
  const traceparent = firstHeaderValue(headers.traceparent);
  return {
    ...(requestId === undefined ? {} : { requestId }),
    ...(correlationId === undefined ? {} : { correlationId }),
    ...(causationId === undefined ? {} : { causationId }),
    ...(providerName === undefined ? {} : { providerName }),
    ...(moduleName === undefined ? {} : { moduleName }),
    ...(operationId === undefined ? {} : { operationId }),
    ...(operationKind === undefined ? {} : { operationKind }),
    ...(actorKind === undefined ? {} : { actorKind }),
    ...(traceparent === undefined ? {} : { traceparent }),
  };
};

export interface ModuleHttpHandlerContext {
  body: unknown;
  params: Record<string, string>;
  request: IncomingMessage;
  url: URL;
}

export type ModuleHttpHandlerResult =
  | unknown
  | {
      body: unknown;
      statusCode?: number;
    };

export type ModuleHttpHandler = (
  context: ModuleHttpHandlerContext
) => ModuleHttpHandlerResult | Promise<ModuleHttpHandlerResult>;

export interface ModuleRuntimeRetryPolicy {
  max_attempts: number;
  initial_delay_ms: number;
}

export interface ModuleRuntimeFunctionDeclaration {
  name: string;
  version: number;
  queue: string;
  input_schema?: string;
  operation?: ServiceOperationMetadata;
  retry_policy?: ModuleRuntimeRetryPolicy;
}

export interface ModuleRuntimeFunctionOptions {
  version?: number;
  queue?: string;
  inputSchema?: string;
  operation?: ServiceOperationMetadata;
  retryPolicy?: ModuleRuntimeRetryPolicy;
}

export interface ModuleRuntimeInvokeRequest {
  request_id: string;
  function_run_id: string;
  function_name: string;
  attempt: number;
  correlation_id: string;
  causation_id?: string | null;
  actor: unknown;
  trace: unknown;
  input: unknown;
}

export interface ModuleRuntimeHandlerContext {
  input: unknown;
  invocation: ModuleRuntimeInvokeRequest;
  request: IncomingMessage;
}

export type ModuleRuntimeHandler = (
  context: ModuleRuntimeHandlerContext
) => unknown | Promise<unknown>;

export interface ModuleEventSurface {
  handlers: readonly ModuleEventHandlerDeclaration[];
}

export interface ModuleEventHandlerDeclaration {
  name: string;
  event_name: string;
  operation?: ServiceOperationMetadata;
}

export interface ModuleEventHandlerOptions {
  operation?: ServiceOperationMetadata;
}

export interface ModuleEventHandleRequest {
  request_id: string;
  outbox_event_id: string;
  handler_name: string;
  event_name: string;
  event_version: number;
  source_module: string;
  aggregate_type: string;
  aggregate_id: string;
  correlation_id: string;
  causation_id?: string | null;
  occurred_at: string;
  actor: unknown;
  trace: unknown;
  payload: unknown;
  headers: unknown;
}

export interface ModuleEventResultAction {
  type: "enqueue_function";
  function_name: string;
  input: unknown;
}

export interface ModuleEventHandleResponse {
  actions?: readonly ModuleEventResultAction[];
}

export interface ModuleEventHandlerContext {
  event: ModuleEventHandleRequest;
  request: IncomingMessage;
}

export type ModuleEventHandler = (
  context: ModuleEventHandlerContext
) =>
  | ModuleEventHandleResponse
  | undefined
  | Promise<ModuleEventHandleResponse | undefined>;

export interface ModuleLifecycleStartupCheck {
  name: string;
  required?: boolean;
  kind: "function_registered" | "capability_declared";
  function_name?: string;
  capability?: string;
}

export interface ModuleLifecycleActivationJob {
  name: string;
  function_name: string;
  run_policy?: "every_startup";
  input?: unknown;
  required?: boolean;
}

export interface ModuleLifecycleSurface {
  startup_checks: readonly ModuleLifecycleStartupCheck[];
  activation_jobs: readonly ModuleLifecycleActivationJob[];
}

export interface ModuleLifecycleActivationOptions {
  input?: unknown;
  required?: boolean;
}

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

export interface AdminSchema {
  entities: readonly SchemaEntity[];
}

export interface SchemaAdminSurface extends AdminSchema {
  kind: "schema";
}

export type AdminActionDangerLevel = "low" | "medium" | "high";

export interface AdminActionInputField {
  name: string;
  label: string;
  field_type: SchemaFieldType;
  required: boolean;
  description?: string;
}

export interface AdminActionInputSchema {
  fields: readonly AdminActionInputField[];
}

export interface AdminActionConfirmation {
  message: string;
  required_phrase?: string;
}

export interface AdminAction {
  name: string;
  label: string;
  capability: string;
  input_schema?: AdminActionInputSchema;
  confirmation?: AdminActionConfirmation;
  danger_level?: AdminActionDangerLevel;
  operation?: ServiceOperationMetadata;
}

export interface AdminMetricBinding {
  label: string;
  value_path: string;
}

export type AdminDeclarativeComponent =
  | {
      kind: "metric_strip";
      metrics: readonly AdminMetricBinding[];
    }
  | {
      kind: "query_value";
      query: string;
      capability: string;
      value_path: string;
    }
  | {
      kind: "entity_table";
      entity: string;
    }
  | {
      kind: "entity_detail";
      entity: string;
    };

export interface AdminDeclarativeSection {
  name: string;
  label: string;
  component: AdminDeclarativeComponent;
}

export interface AdminDeclarativePage {
  name: string;
  label: string;
  sections: readonly AdminDeclarativeSection[];
}

export interface AdminDeclarativeSurface {
  kind: "declarative_custom";
  pages: readonly AdminDeclarativePage[];
  actions: readonly AdminAction[];
  fallback_schema?: AdminSchema;
}

export type AdminEmbeddedRuntime = "iframe" | "wasm" | "js_bundle";

export interface AdminEmbeddedSurface {
  kind: "embedded_custom";
  runtime: AdminEmbeddedRuntime;
  entry: {
    kind: "url";
    url: string;
    allowed_origins?: readonly string[];
  };
  sandbox: {
    allow_scripts?: boolean;
    allow_forms?: boolean;
    allow_popups?: boolean;
    allow_same_origin?: boolean;
  };
  permissions?: readonly (
    | {
        kind: "read_entity";
        entity: string;
      }
    | {
        kind: "invoke_action";
        action: string;
      }
  )[];
  fallback_schema?: AdminSchema;
}

export interface ProviderModuleDefinition {
  name: string;
  version?: string;
  compatibility?: ServiceModuleCompatibility;
  service?: ServiceModuleProviderMetadata;
  storyDisplay?: readonly ModuleStoryDisplayDescriptor[];
  capabilities?: readonly string[];
  dependencies?: readonly string[];
  httpRoutes?: readonly ModuleHttpRoute[];
  runtimeFunctions?: readonly ModuleRuntimeFunctionDeclaration[];
  eventHandlers?: readonly ModuleEventHandlerDeclaration[];
  lifecycle?: ModuleLifecycleSurface;
  admin?: unknown | null;
  console?: readonly ModuleConsoleSurface[];
}

export type ServiceModuleDefinition = Omit<
  ProviderModuleDefinition,
  "compatibility" | "service"
>;

export type ServiceModuleManifest = Omit<
  ProviderModuleManifest,
  "compatibility" | "service" | "source"
>;

export interface ServiceInstallCommand {
  command: string;
  cwd?: string;
}

export interface ServiceInstallService {
  name?: string;
  command: string;
  cwd?: string;
  readyUrl?: string;
  readyTimeoutMs?: number;
  autoStart?: boolean;
}

export interface ServiceInstall {
  env?: Record<string, string>;
  commands?: readonly (string | ServiceInstallCommand)[];
  services?: readonly ServiceInstallService[];
}

export interface ServiceDefinition {
  name: string;
  version?: string;
  compatibility?: ServiceModuleCompatibility;
  deployment?: ServiceModuleDeploymentMetadata;
  install?: ServiceInstall;
  modules: readonly ServiceModuleManifest[];
  requiredEnv?: readonly string[];
  statusPath?: string;
  statusUrl?: string;
  transports?: readonly string[];
}

export interface ServiceManifest {
  name: string;
  version: string;
  protocol: "lenso.service.v1";
  compatibility?: ServiceModuleCompatibility;
  deployment?: ServiceModuleDeploymentMetadata;
  install?: ServiceInstall;
  modules: readonly ServiceModuleManifest[];
  required_env: readonly string[];
  status_path: string;
  status_url?: string;
  transports: readonly string[];
}

export const systemPlaneCorePath = "/system-plane/v1" as const;
export const systemPlaneCoreProtocol = "lenso.system-plane.v1" as const;

export interface ProviderCoreIdentity {
  serviceId: string;
  servicePrincipal: string;
  serviceRevision: string;
}

export interface ProviderCoreOptions extends ProviderCoreIdentity {
  bearerToken: string;
}

export interface SystemPlaneCoreDocument extends ProviderCoreIdentity {
  protocol: typeof systemPlaneCoreProtocol;
}

export interface ModuleAdminPage {
  records: readonly unknown[];
  next_cursor?: string | null;
}

export interface ModuleAdminDataSource {
  list: (query: {
    limit: number;
    cursor?: string;
  }) => ModuleAdminPage | Promise<ModuleAdminPage>;
  detail: (
    id: string
  ) => unknown | null | undefined | Promise<unknown | null | undefined>;
}

export type ModuleAdminQueryHandler = (context: {
  query: string;
  request: IncomingMessage;
}) => unknown | Promise<unknown>;

export interface ModuleAdminActionHandlerContext {
  action: string;
  input: unknown;
  request: IncomingMessage;
}

export type ModuleAdminActionHandler = (
  context: ModuleAdminActionHandlerContext
) => unknown | Promise<unknown>;

export interface ServedModuleProvider {
  baseUrl: string;
  manifestUrl: string;
  statusUrl: string;
  server: HttpServer | Http2Server;
  close: () => Promise<void>;
}

export interface ServedService extends ServedModuleProvider {
  systemPlaneCoreUrl?: string;
}

export type ServiceModuleHandlers = Pick<
  ServeModuleProviderOptions,
  "actions" | "data" | "events" | "http" | "queries" | "runtime"
>;

export interface ServeModuleProviderOptions {
  host?: string;
  port?: number;
  basePath?: string;
  data?: Record<string, ModuleAdminDataSource>;
  queries?: Record<string, ModuleAdminQueryHandler>;
  actions?: Record<string, ModuleAdminActionHandler>;
  http?: Record<string, ModuleHttpHandler>;
  runtime?: Record<string, ModuleRuntimeHandler>;
  events?: Record<string, ModuleEventHandler>;
  status?: ModuleProviderStatusOptions;
  onReady?: (server: ServedModuleProvider) => void;
}

export interface ServeServiceOptions {
  host?: string;
  port?: number;
  basePath?: string;
  modules?: Record<string, ServiceModuleHandlers>;
  providerCore?: ProviderCoreOptions;
  status?: ServiceStatusOptions;
  onReady?: (server: ServedService) => void;
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

const providerCoreDocument = (
  options: ProviderCoreOptions | undefined,
  host: string
): SystemPlaneCoreDocument | undefined => {
  if (!options) {
    return undefined;
  }
  if (host !== "127.0.0.1" && host !== "localhost") {
    throw new Error("providerCore requires a loopback host");
  }
  for (const field of [
    "serviceId",
    "servicePrincipal",
    "serviceRevision",
    "bearerToken",
  ] as const) {
    const value = options[field];
    if (typeof value !== "string" || value.trim().length === 0) {
      throw new Error(`providerCore.${field} must be a non-empty string`);
    }
  }
  return {
    protocol: systemPlaneCoreProtocol,
    serviceId: options.serviceId,
    servicePrincipal: options.servicePrincipal,
    serviceRevision: options.serviceRevision,
  };
};

const localBearerMatches = (
  request: IncomingMessage,
  expectedToken: Buffer
) => {
  const authorization = request.headers.authorization;
  const candidate =
    typeof authorization === "string" && authorization.startsWith("Bearer ")
      ? authorization.slice("Bearer ".length)
      : "";
  const candidateBytes = Buffer.from(candidate, "utf8");
  const paddedCandidate = Buffer.alloc(expectedToken.length);
  candidateBytes.copy(paddedCandidate, 0, 0, expectedToken.length);
  const contentsMatch = timingSafeEqual(paddedCandidate, expectedToken);
  return candidateBytes.length === expectedToken.length && contentsMatch;
};

const GRPC_PATHS = {
  getAdminRecord: "/lenso.service.module.v1.ServiceModule/GetAdminRecord",
  getManifest: "/lenso.service.module.v1.ServiceModule/GetManifest",
  handleEvent: "/lenso.service.module.v1.ServiceModule/HandleEvent",
  invokeAdminAction: "/lenso.service.module.v1.ServiceModule/InvokeAdminAction",
  invokeFunction: "/lenso.service.module.v1.ServiceModule/InvokeFunction",
  listAdminRecords: "/lenso.service.module.v1.ServiceModule/ListAdminRecords",
  proxyHttpRoute: "/lenso.service.module.v1.ServiceModule/ProxyHttpRoute",
  queryAdminValue: "/lenso.service.module.v1.ServiceModule/QueryAdminValue",
} as const;

const grpcStatus = {
  invalidArgument: "3",
  notFound: "5",
  ok: "0",
  unimplemented: "12",
} as const;

const writeGrpcResponse = (
  stream: ServerHttp2Stream,
  status: string,
  payload?: unknown,
  message?: string
) => {
  if (payload === undefined) {
    stream.respond({
      ":status": 200,
      "content-type": "application/grpc",
      "grpc-status": status,
      ...(message ? { "grpc-message": encodeURIComponent(message) } : {}),
    });
    stream.end();
    return;
  }

  stream.respond(
    {
      ":status": 200,
      "content-type": "application/grpc",
    },
    { waitForTrailers: true }
  );
  stream.on("wantTrailers", () => {
    stream.sendTrailers({
      "grpc-status": status,
      ...(message ? { "grpc-message": encodeURIComponent(message) } : {}),
    });
  });
  stream.end(grpcFrame(payload));
};

function grpcFrame(payload: unknown) {
  const message = encodeJsonEnvelope(JSON.stringify(payload));
  const frame = Buffer.alloc(5 + message.length);
  frame[0] = 0;
  frame.writeUInt32BE(message.length, 1);
  message.copy(frame, 5);
  return frame;
}

function readGrpcPayload(body: Buffer) {
  if (body.length < 5 || body[0] !== 0) {
    throw new Error("invalid gRPC frame");
  }
  const length = body.readUInt32BE(1);
  const message = body.subarray(5, 5 + length);
  return JSON.parse(decodeJsonEnvelope(message));
}

function encodeJsonEnvelope(payloadJson: string) {
  const payload = Buffer.from(payloadJson, "utf-8");
  return Buffer.concat([
    Buffer.from([0x0a]),
    encodeVarint(payload.length),
    payload,
  ]);
}

function decodeJsonEnvelope(message: Buffer) {
  if (message[0] !== 0x0a) {
    throw new Error("invalid JsonEnvelope");
  }
  const { value: length, offset } = decodeVarint(message, 1);
  return message.subarray(offset, offset + length).toString("utf-8");
}

function encodeVarint(value: number) {
  const bytes: number[] = [];
  let current = value;
  do {
    let byte = current % 128;
    current = Math.floor(current / 128);
    if (current > 0) {
      byte += 128;
    }
    bytes.push(byte);
  } while (current > 0);
  return Buffer.from(bytes);
}

function decodeVarint(buffer: Buffer, offset: number) {
  let value = 0;
  let shift = 0;
  let index = offset;
  while (index < buffer.length) {
    const byte = buffer[index];
    if (byte === undefined) {
      break;
    }
    value += (byte % 128) * 2 ** shift;
    index += 1;
    if (byte < 128) {
      return { offset: index, value };
    }
    shift += 7;
  }
  throw new Error("unterminated varint");
}

const route = (
  method: ModuleHttpMethod,
  path: string,
  options: ModuleHttpRouteOptions = {}
): ModuleHttpRoute => ({
  ...(options.capability ? { capability: options.capability } : {}),
  ...(options.displayName ? { display_name: options.displayName } : {}),
  method,
  ...(options.operation ? { operation: options.operation } : {}),
  path,
  ...(options.storyTitle ? { story_title: options.storyTitle } : {}),
});

const routeKey = (method: ModuleHttpMethod, path: string) =>
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
  result: ModuleHttpHandlerResult
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
  handlers: Record<string, ModuleHttpHandler>;
  manifest: ProviderModuleManifest;
  request: IncomingMessage;
}): Promise<{ body: unknown; statusCode: number } | null> => {
  const method = request.method as ModuleHttpMethod | undefined;
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

const runtimeFunctionQueue = (name: string) => name.split(".")[0] ?? name;

const handleRuntimeFunctionRequest = async ({
  basePath,
  handlers,
  request,
}: {
  basePath: string;
  handlers: Record<string, ModuleRuntimeHandler>;
  request: IncomingMessage;
}): Promise<{ body: unknown; statusCode: number } | null> => {
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
  const invocation = (await readBody(request)) as ModuleRuntimeInvokeRequest;
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

const handleEventRequest = async ({
  basePath,
  handlers,
  request,
}: {
  basePath: string;
  handlers: Record<string, ModuleEventHandler>;
  request: IncomingMessage;
}): Promise<{ body: unknown; statusCode: number } | null> => {
  if (request.method !== "POST") {
    return null;
  }
  const url = new URL(request.url ?? "", "http://127.0.0.1");
  const prefix = `${basePath}/events/handlers/`;
  if (!(url.pathname.startsWith(prefix) && url.pathname.endsWith("/invoke"))) {
    return null;
  }
  const handlerName = decodeURIComponent(
    url.pathname.slice(prefix.length, -"/invoke".length)
  );
  if (!handlerName || handlerName.includes("/")) {
    return {
      body: {
        error: {
          code: "not_found",
          message: "event handler endpoint not found",
        },
      },
      statusCode: 404,
    };
  }
  const handler = handlers[handlerName];
  if (!handler) {
    return {
      body: {
        error: {
          code: "not_found",
          message: `${handlerName} event handler not found`,
        },
      },
      statusCode: 404,
    };
  }
  const event = (await readBody(request)) as ModuleEventHandleRequest;
  const result = await handler({ event, request });
  return {
    body: result ?? { actions: [] },
    statusCode: 200,
  };
};

const invokeEventHandler = async (
  handlers: Record<string, ModuleEventHandler>,
  event: ModuleEventHandleRequest
) => {
  const handlerName = event.handler_name;
  const handler = handlers[handlerName];
  if (!handler) {
    throw new Error(`${handlerName} event handler not found`);
  }
  return (
    (await handler({
      event,
      request: undefined as unknown as IncomingMessage,
    })) ?? { actions: [] }
  );
};

const handleAdminActionRequest = async ({
  basePath,
  handlers,
  request,
}: {
  basePath: string;
  handlers: Record<string, ModuleAdminActionHandler>;
  request: IncomingMessage;
}): Promise<{ body: unknown; statusCode: number } | null> => {
  if (request.method !== "POST") {
    return null;
  }
  const url = new URL(request.url ?? "", "http://127.0.0.1");
  const prefix = `${basePath}/http/admin/actions/`;
  if (!url.pathname.startsWith(prefix)) {
    return null;
  }
  const action = decodeURIComponent(url.pathname.slice(prefix.length));
  if (!action || action.includes("/")) {
    return {
      body: {
        error: {
          code: "not_found",
          message: "admin action endpoint not found",
        },
      },
      statusCode: 404,
    };
  }
  const handler = handlers[action];
  if (!handler) {
    return {
      body: {
        error: {
          code: "not_found",
          message: `${action} admin action handler not found`,
        },
      },
      statusCode: 404,
    };
  }
  const input = await readBody(request);
  const result = await handler({
    action,
    input,
    request,
  });
  return {
    body: { result: result ?? null },
    statusCode: 200,
  };
};

const handleAdminQueryRequest = async ({
  basePath,
  handlers,
  request,
}: {
  basePath: string;
  handlers: Record<string, ModuleAdminQueryHandler>;
  request: IncomingMessage;
}): Promise<{ body: unknown; statusCode: number } | null> => {
  if (request.method !== "GET") {
    return null;
  }
  const url = new URL(request.url ?? "", "http://127.0.0.1");
  const prefix = `${basePath}/http/admin/queries/`;
  if (!url.pathname.startsWith(prefix)) {
    return null;
  }
  const query = decodeURIComponent(url.pathname.slice(prefix.length));
  if (!query || query.includes("/")) {
    return {
      body: {
        error: {
          code: "not_found",
          message: "admin query endpoint not found",
        },
      },
      statusCode: 404,
    };
  }
  const handler = handlers[query];
  if (!handler) {
    return {
      body: {
        error: {
          code: "not_found",
          message: `${query} admin query handler not found`,
        },
      },
      statusCode: 404,
    };
  }
  const data = await handler({ query, request });
  return {
    body: { data: data ?? null },
    statusCode: 200,
  };
};

interface FieldOptions {
  label?: string;
  nullable?: boolean;
}

export interface ActionFieldOptions {
  label?: string;
  required?: boolean;
  description?: string;
}

export interface AdminActionOptions {
  label?: string;
  capability: string;
  inputFields?: readonly AdminActionInputField[];
  confirmation?: AdminActionConfirmation;
  dangerLevel?: AdminActionDangerLevel;
  operation?: ServiceOperationMetadata;
}

export interface AdminConfirmationOptions {
  requiredPhrase?: string;
}

export interface AdminDeclarativePageOptions {
  label?: string;
  sections?: readonly AdminDeclarativeSection[];
}

export interface AdminDeclarativeSectionOptions {
  label?: string;
  component: AdminDeclarativeComponent;
}

export interface AdminDeclarativeSurfaceOptions {
  pages?: readonly AdminDeclarativePage[];
  actions?: readonly AdminAction[];
  fallbackSchema?: AdminSchema;
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

const actionField = (
  name: string,
  fieldType: SchemaFieldType,
  options: ActionFieldOptions
): AdminActionInputField => ({
  ...(options.description ? { description: options.description } : {}),
  field_type: fieldType,
  label: options.label ?? titleCase(name),
  name,
  required: options.required ?? false,
});

const handleAdminDataRequest = async ({
  basePath,
  data,
  requestUrl,
}: {
  basePath: string;
  data: Record<string, ModuleAdminDataSource>;
  requestUrl: string;
}): Promise<{ body: unknown; statusCode: number } | null> => {
  const url = new URL(requestUrl, "http://127.0.0.1");
  const prefix = `${basePath}/http/admin/`;
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

export const defineProviderModule = (
  definition: ProviderModuleDefinition
): ProviderModuleManifest => {
  if (!definition.name.trim()) {
    throw new Error("Service module name is required");
  }
  return {
    admin: definition.admin ?? null,
    capabilities: definition.capabilities ?? [],
    ...(definition.compatibility
      ? { compatibility: definition.compatibility }
      : {}),
    console: definition.console ?? [],
    dependencies: definition.dependencies ?? [],
    ...(definition.eventHandlers
      ? { events: { handlers: definition.eventHandlers } }
      : {}),
    http_routes: definition.httpRoutes ?? [],
    ...(definition.lifecycle ? { lifecycle: definition.lifecycle } : {}),
    name: definition.name,
    runtime: {
      functions: definition.runtimeFunctions ?? [],
    },
    ...(definition.service ? { service: definition.service } : {}),
    source: "service",
    story_display: definition.storyDisplay ?? [],
    version: definition.version ?? "0.1.0",
  };
};

export const defineModule = (
  definition: ServiceModuleDefinition
): ServiceModuleManifest => {
  const {
    compatibility: _compatibility,
    service: _service,
    source: _source,
    ...module
  } = defineProviderModule(definition);
  return module;
};

export const defineService = (
  definition: ServiceDefinition
): ServiceManifest => {
  const name = definition.name.trim();
  if (!name) {
    throw new Error("Service name is required");
  }
  if (definition.modules.length === 0) {
    throw new Error("Service must provide at least one module");
  }
  const moduleNames = new Set<string>();
  for (const module of definition.modules) {
    if (!module.name.trim()) {
      throw new Error("Service module name is required");
    }
    if (moduleNames.has(module.name)) {
      throw new Error(`Duplicate service module: ${module.name}`);
    }
    moduleNames.add(module.name);
  }
  return {
    ...(definition.compatibility
      ? { compatibility: definition.compatibility }
      : {}),
    ...(definition.deployment ? { deployment: definition.deployment } : {}),
    ...(definition.install ? { install: definition.install } : {}),
    modules: definition.modules,
    name,
    protocol: "lenso.service.v1",
    required_env: definition.requiredEnv ?? [],
    status_path: definition.statusPath ?? "/lenso/service/v1/status",
    ...(definition.statusUrl ? { status_url: definition.statusUrl } : {}),
    transports: definition.transports ?? ["http"],
    version: definition.version ?? "0.1.0",
  };
};

export const getRoute = (path: string, options: ModuleHttpRouteOptions = {}) =>
  route("GET", path, options);

export const postRoute = (path: string, options: ModuleHttpRouteOptions = {}) =>
  route("POST", path, options);

export const putRoute = (path: string, options: ModuleHttpRouteOptions = {}) =>
  route("PUT", path, options);

export const patchRoute = (
  path: string,
  options: ModuleHttpRouteOptions = {}
) => route("PATCH", path, options);

export const deleteRoute = (
  path: string,
  options: ModuleHttpRouteOptions = {}
) => route("DELETE", path, options);

export const runtimeFunction = (
  name: string,
  options: ModuleRuntimeFunctionOptions = {}
): ModuleRuntimeFunctionDeclaration => ({
  ...(options.inputSchema ? { input_schema: options.inputSchema } : {}),
  ...(options.operation ? { operation: options.operation } : {}),
  queue: options.queue ?? runtimeFunctionQueue(name),
  ...(options.retryPolicy ? { retry_policy: options.retryPolicy } : {}),
  name,
  version: options.version ?? 1,
});

export const eventHandler = (
  name: string,
  eventName: string,
  options: ModuleEventHandlerOptions = {}
): ModuleEventHandlerDeclaration => ({
  event_name: eventName,
  name,
  ...(options.operation ? { operation: options.operation } : {}),
});

export const everyStartup = (
  name: string,
  functionName: string,
  options: ModuleLifecycleActivationOptions = {}
): ModuleLifecycleActivationJob => ({
  function_name: functionName,
  input: options.input ?? {},
  name,
  required: options.required ?? true,
  run_policy: "every_startup",
});

export const lifecycle = ({
  activationJobs,
  startupChecks,
}: {
  startupChecks?: readonly ModuleLifecycleStartupCheck[];
  activationJobs?: readonly ModuleLifecycleActivationJob[];
}): ModuleLifecycleSurface => ({
  activation_jobs: activationJobs ?? [],
  startup_checks: startupChecks ?? [],
});

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

export const actionTextField = (
  name: string,
  options: ActionFieldOptions = {}
) => actionField(name, { kind: "string" }, options);

export const actionIntegerField = (
  name: string,
  options: ActionFieldOptions = {}
) => actionField(name, { kind: "integer" }, options);

export const actionBooleanField = (
  name: string,
  options: ActionFieldOptions = {}
) => actionField(name, { kind: "boolean" }, options);

export const actionTimestampField = (
  name: string,
  options: ActionFieldOptions = {}
) => actionField(name, { kind: "timestamp" }, options);

export const actionJsonField = (
  name: string,
  options: ActionFieldOptions = {}
) => actionField(name, { kind: "json" }, options);

export const actionConfirmation = (
  message: string,
  options: AdminConfirmationOptions = {}
): AdminActionConfirmation => ({
  message,
  ...(options.requiredPhrase
    ? { required_phrase: options.requiredPhrase }
    : {}),
});

export const adminAction = (
  name: string,
  options: AdminActionOptions
): AdminAction => ({
  capability: options.capability,
  ...(options.confirmation ? { confirmation: options.confirmation } : {}),
  ...(options.dangerLevel && options.dangerLevel !== "low"
    ? { danger_level: options.dangerLevel }
    : {}),
  ...(options.inputFields?.length
    ? { input_schema: { fields: options.inputFields } }
    : {}),
  label: options.label ?? titleCase(name),
  name,
  ...(options.operation ? { operation: options.operation } : {}),
});

export const metricBinding = (
  label: string,
  valuePath: string
): AdminMetricBinding => ({
  label,
  value_path: valuePath,
});

export const metricStrip = (
  metrics: readonly AdminMetricBinding[]
): AdminDeclarativeComponent => ({
  kind: "metric_strip",
  metrics,
});

export const queryValue = (
  query: string,
  options: { capability: string; valuePath: string }
): AdminDeclarativeComponent => ({
  capability: options.capability,
  kind: "query_value",
  query,
  value_path: options.valuePath,
});

export const entityTable = (entity: string): AdminDeclarativeComponent => ({
  entity,
  kind: "entity_table",
});

export const entityDetail = (entity: string): AdminDeclarativeComponent => ({
  entity,
  kind: "entity_detail",
});

export const declarativeSection = (
  name: string,
  options: AdminDeclarativeSectionOptions
): AdminDeclarativeSection => ({
  component: options.component,
  label: options.label ?? titleCase(name),
  name,
});

export const declarativePage = (
  name: string,
  options: AdminDeclarativePageOptions = {}
): AdminDeclarativePage => ({
  label: options.label ?? titleCase(name),
  name,
  sections: options.sections ?? [],
});

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

export const adminSchema = (
  entities: readonly SchemaEntity[]
): AdminSchema => ({
  entities,
});

export const schemaAdmin = (
  entities: readonly SchemaEntity[]
): SchemaAdminSurface => ({
  ...adminSchema(entities),
  kind: "schema",
});

export const declarativeCustom = (
  options: AdminDeclarativeSurfaceOptions = {}
): AdminDeclarativeSurface => ({
  actions: options.actions ?? [],
  ...(options.fallbackSchema
    ? { fallback_schema: options.fallbackSchema }
    : {}),
  kind: "declarative_custom",
  pages: options.pages ?? [],
});

export const embeddedCustom = (
  surface: Omit<AdminEmbeddedSurface, "kind">
): AdminEmbeddedSurface => ({
  ...surface,
  kind: "embedded_custom",
});

const moduleProviderStatusChecks = async (
  options: ModuleProviderStatusOptions | undefined
) => {
  if (!options?.checks) {
    return [{ name: "service", status: "ok" as const }];
  }
  return typeof options.checks === "function"
    ? await options.checks()
    : options.checks;
};

const moduleProviderStatusResponse = async ({
  baseUrl,
  manifest,
  options,
}: {
  baseUrl: string;
  manifest: ProviderModuleManifest;
  options: ModuleProviderStatusOptions | undefined;
}): Promise<ModuleProviderStatus> => ({
  checks: await moduleProviderStatusChecks(options),
  manifestUrl: `${baseUrl}/manifest`,
  moduleName: manifest.name,
  protocolVersion: "1",
  serviceName: manifest.service?.name ?? "api",
  state: options?.state ?? "ready",
  transports: manifest.service?.transports ?? ["http"],
  version: manifest.service?.version ?? manifest.version,
});

const serviceStatusChecks = async (
  options: ServiceStatusOptions | undefined
) => {
  if (!options?.checks) {
    return [{ name: "service", status: "ok" as const }];
  }
  return typeof options.checks === "function"
    ? await options.checks()
    : options.checks;
};

const serviceStatusResponse = async ({
  baseUrl,
  manifest,
  options,
}: {
  baseUrl: string;
  manifest: ServiceManifest;
  options: ServiceStatusOptions | undefined;
}): Promise<ServiceStatus> => ({
  checks: await serviceStatusChecks(options),
  manifestUrl: `${baseUrl}/manifest`,
  modules: manifest.modules.map((module) => ({
    name: module.name,
    version: module.version,
  })),
  protocolVersion: "1",
  serviceName: manifest.name,
  state: options?.state ?? "ready",
  transports: manifest.transports,
  version: manifest.version,
});

const providerManifestForServiceModule = (
  service: ServiceManifest,
  module: ServiceModuleManifest
): ProviderModuleManifest => ({
  ...module,
  ...(service.compatibility ? { compatibility: service.compatibility } : {}),
  service: {
    ...(service.deployment ? { deployment: service.deployment } : {}),
    name: service.name,
    required_env: service.required_env,
    status_path: service.status_path,
    ...(service.status_url ? { status_url: service.status_url } : {}),
    transports: service.transports,
    version: service.version,
  },
  source: "service",
});

export const serveModuleProvider = async (
  manifest: ProviderModuleManifest,
  options: ServeModuleProviderOptions = {}
): Promise<ServedModuleProvider> => {
  const host = options.host ?? "127.0.0.1";
  const port = options.port ?? 4100;
  const basePath = normalizeBasePath(options.basePath ?? "/lenso/module/v1");
  const manifestPath = `${basePath}/manifest`;
  const statusPath = `${basePath}/status`;
  let servedBaseUrl = "";

  const server = createServer(async (request, response) => {
    const requestPath = new URL(request.url ?? "", "http://127.0.0.1").pathname;
    if (request.method === "GET" && requestPath === manifestPath) {
      sendJson(response, 200, manifest);
      return;
    }
    if (request.method === "GET" && requestPath === statusPath) {
      sendJson(
        response,
        200,
        await moduleProviderStatusResponse({
          baseUrl: servedBaseUrl,
          manifest,
          options: options.status,
        })
      );
      return;
    }
    if (request.method === "GET") {
      const queryResult = await handleAdminQueryRequest({
        basePath,
        handlers: options.queries ?? {},
        request,
      });
      if (queryResult) {
        sendJson(response, queryResult.statusCode, queryResult.body);
        return;
      }
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
    const actionResult = await handleAdminActionRequest({
      basePath,
      handlers: options.actions ?? {},
      request,
    });
    if (actionResult) {
      sendJson(response, actionResult.statusCode, actionResult.body);
      return;
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
    const eventResult = await handleEventRequest({
      basePath,
      handlers: options.events ?? {},
      request,
    });
    if (eventResult) {
      sendJson(response, eventResult.statusCode, eventResult.body);
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
        message: `${manifest.name} service module endpoint not found`,
      },
    });
  });

  server.listen(port, host);
  await once(server, "listening");

  const address = server.address();
  const boundPort =
    typeof address === "object" && address ? address.port : port;
  const baseUrl = `http://${host}:${boundPort}${basePath}`;
  servedBaseUrl = baseUrl;
  const served = {
    baseUrl,
    close: async () => {
      server.close();
      await once(server, "close");
    },
    manifestUrl: `${baseUrl}/manifest`,
    server,
    statusUrl: `${baseUrl}/status`,
  } satisfies ServedModuleProvider;

  options.onReady?.(served);
  return served;
};

// ponytail: shared server wrapper is intentionally flat; split handlers if this grows again.
// eslint-disable-next-line complexity
export const serveService = async (
  manifest: ServiceManifest,
  options: ServeServiceOptions = {}
): Promise<ServedService> => {
  const host = options.host ?? "127.0.0.1";
  const port = options.port ?? 4100;
  const basePath = normalizeBasePath(options.basePath ?? "/lenso/service/v1");
  const manifestPath = `${basePath}/manifest`;
  const statusPath = `${basePath}/status`;
  const coreDocument = providerCoreDocument(options.providerCore, host);
  const coreBearer = options.providerCore
    ? Buffer.from(options.providerCore.bearerToken, "utf8")
    : undefined;
  let servedBaseUrl = "";

  const server = createServer(async (request, response) => {
    const requestPath = new URL(request.url ?? "", "http://127.0.0.1").pathname;
    if (
      request.method === "GET" &&
      requestPath === systemPlaneCorePath &&
      coreDocument &&
      coreBearer
    ) {
      if (!localBearerMatches(request, coreBearer)) {
        sendJson(response, 401, {
          error: {
            code: "system_plane_bearer_required",
            message:
              "System Plane Core access requires the configured local bearer credential",
          },
        });
        return;
      }
      sendJson(response, 200, coreDocument);
      return;
    }
    if (request.method === "GET" && requestPath === manifestPath) {
      sendJson(response, 200, manifest);
      return;
    }
    if (request.method === "GET" && requestPath === statusPath) {
      sendJson(
        response,
        200,
        await serviceStatusResponse({
          baseUrl: servedBaseUrl,
          manifest,
          options: options.status,
        })
      );
      return;
    }

    for (const module of manifest.modules) {
      const moduleBasePath = `${basePath}/modules/${module.name}`;
      const moduleHandlers = options.modules?.[module.name] ?? {};
      const providerManifest = providerManifestForServiceModule(
        manifest,
        module
      );

      if (
        request.method === "GET" &&
        requestPath === `${moduleBasePath}/manifest`
      ) {
        sendJson(response, 200, module);
        return;
      }
      if (request.method === "GET") {
        const queryResult = await handleAdminQueryRequest({
          basePath: moduleBasePath,
          handlers: moduleHandlers.queries ?? {},
          request,
        });
        if (queryResult) {
          sendJson(response, queryResult.statusCode, queryResult.body);
          return;
        }
        const adminResult = await handleAdminDataRequest({
          basePath: moduleBasePath,
          data: moduleHandlers.data ?? {},
          requestUrl: request.url ?? "",
        });
        if (adminResult) {
          sendJson(response, adminResult.statusCode, adminResult.body);
          return;
        }
      }
      const actionResult = await handleAdminActionRequest({
        basePath: moduleBasePath,
        handlers: moduleHandlers.actions ?? {},
        request,
      });
      if (actionResult) {
        sendJson(response, actionResult.statusCode, actionResult.body);
        return;
      }
      const runtimeResult = await handleRuntimeFunctionRequest({
        basePath: moduleBasePath,
        handlers: moduleHandlers.runtime ?? {},
        request,
      });
      if (runtimeResult) {
        sendJson(response, runtimeResult.statusCode, runtimeResult.body);
        return;
      }
      const eventResult = await handleEventRequest({
        basePath: moduleBasePath,
        handlers: moduleHandlers.events ?? {},
        request,
      });
      if (eventResult) {
        sendJson(response, eventResult.statusCode, eventResult.body);
        return;
      }
      const httpResult = await handleHttpRouteRequest({
        basePath: moduleBasePath,
        handlers: moduleHandlers.http ?? {},
        manifest: providerManifest,
        request,
      });
      if (httpResult) {
        sendJson(response, httpResult.statusCode, httpResult.body);
        return;
      }
    }

    sendJson(response, 404, {
      error: {
        code: "not_found",
        message: `${manifest.name} service endpoint not found`,
      },
    });
  });

  server.listen(port, host);
  await once(server, "listening");

  const address = server.address();
  const boundPort =
    typeof address === "object" && address ? address.port : port;
  const baseUrl = `http://${host}:${boundPort}${basePath}`;
  const systemPlaneCoreUrl = coreDocument
    ? `http://${host}:${boundPort}${systemPlaneCorePath}`
    : undefined;
  servedBaseUrl = baseUrl;
  const served = {
    baseUrl,
    close: async () => {
      server.close();
      await once(server, "close");
    },
    manifestUrl: `${baseUrl}/manifest`,
    server,
    statusUrl: `${baseUrl}/status`,
    ...(systemPlaneCoreUrl ? { systemPlaneCoreUrl } : {}),
  } satisfies ServedService;

  options.onReady?.(served);
  return served;
};

export const serveModuleProviderGrpc = async (
  manifest: ProviderModuleManifest,
  options: ServeModuleProviderOptions = {}
): Promise<ServedModuleProvider> => {
  const host = options.host ?? "127.0.0.1";
  const port = options.port ?? 50_051;
  const server = createHttp2Server();

  server.on("stream", (stream, headers) => {
    void handleGrpcStream({
      headers,
      manifest,
      options,
      stream: stream as ServerHttp2Stream,
    });
  });

  server.listen(port, host);
  await once(server, "listening");

  const address = server.address();
  const boundPort =
    typeof address === "object" && address ? address.port : port;
  const baseUrl = `grpc://${host}:${boundPort}`;
  const served = {
    baseUrl,
    close: async () => {
      server.close();
      await once(server, "close");
    },
    manifestUrl: `${baseUrl}${GRPC_PATHS.getManifest}`,
    server,
    statusUrl: `${baseUrl}/lenso.service.module.v1.ServiceModule/GetStatus`,
  } satisfies ServedModuleProvider;

  options.onReady?.(served);
  return served;
};

async function handleGrpcStream({
  headers,
  manifest,
  options,
  stream,
}: {
  headers: NodeJS.Dict<number | string | string[]>;
  manifest: ProviderModuleManifest;
  options: ServeModuleProviderOptions;
  stream: ServerHttp2Stream;
}) {
  const path = headers[":path"];
  if (typeof path !== "string") {
    writeGrpcResponse(
      stream,
      grpcStatus.unimplemented,
      undefined,
      "unknown method"
    );
    return;
  }
  try {
    const payload = readGrpcPayload(await readGrpcBody(stream));
    const response = await handleGrpcPayload(path, payload, manifest, options);
    writeGrpcResponse(stream, grpcStatus.ok, response);
  } catch (error) {
    const message =
      error instanceof Error ? error.message : "gRPC request failed";
    writeGrpcResponse(stream, grpcStatus.invalidArgument, undefined, message);
  }
}

async function readGrpcBody(stream: ServerHttp2Stream) {
  const chunks: Buffer[] = [];
  for await (const chunk of stream) {
    chunks.push(Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk));
  }
  return Buffer.concat(chunks);
}

function handleGrpcPayload(
  path: string,
  payload: Record<string, unknown>,
  manifest: ProviderModuleManifest,
  options: ServeModuleProviderOptions
) {
  switch (path) {
    case GRPC_PATHS.getManifest: {
      return manifest;
    }
    case GRPC_PATHS.listAdminRecords: {
      return listGrpcAdminRecords(payload, options.data ?? {});
    }
    case GRPC_PATHS.getAdminRecord: {
      return getGrpcAdminRecord(payload, options.data ?? {});
    }
    case GRPC_PATHS.invokeAdminAction: {
      return invokeGrpcAdminAction(payload, options.actions ?? {});
    }
    case GRPC_PATHS.queryAdminValue: {
      return invokeGrpcAdminQuery(payload, options.queries ?? {});
    }
    case GRPC_PATHS.proxyHttpRoute: {
      return proxyGrpcHttpRoute(payload, options.http ?? {});
    }
    case GRPC_PATHS.invokeFunction: {
      return invokeGrpcRuntimeFunction(payload, options.runtime ?? {});
    }
    case GRPC_PATHS.handleEvent: {
      return invokeEventHandler(
        options.events ?? {},
        payload as unknown as ModuleEventHandleRequest
      );
    }
    default: {
      throw new Error("unknown gRPC method");
    }
  }
}

function listGrpcAdminRecords(
  payload: Record<string, unknown>,
  data: Record<string, ModuleAdminDataSource>
) {
  const entity = String(payload.entity ?? "");
  const source = data[entity];
  if (!source) {
    throw new Error(`${entity} admin data not found`);
  }
  return source.list({
    limit: Number(payload.limit ?? 50),
    ...(typeof payload.cursor === "string" ? { cursor: payload.cursor } : {}),
  });
}

async function getGrpcAdminRecord(
  payload: Record<string, unknown>,
  data: Record<string, ModuleAdminDataSource>
) {
  const entity = String(payload.entity ?? "");
  const source = data[entity];
  if (!source) {
    throw new Error(`${entity} admin data not found`);
  }
  const record = await source.detail(String(payload.id ?? ""));
  return { record: record ?? null };
}

async function invokeGrpcAdminAction(
  payload: Record<string, unknown>,
  handlers: Record<string, ModuleAdminActionHandler>
) {
  const action = String(payload.action ?? "");
  const handler = handlers[action];
  if (!handler) {
    throw new Error(`${action} admin action handler not found`);
  }
  const result = await handler({
    action,
    input: payload.input,
    request: undefined as unknown as IncomingMessage,
  });
  return { result: result ?? null };
}

async function invokeGrpcAdminQuery(
  payload: Record<string, unknown>,
  handlers: Record<string, ModuleAdminQueryHandler>
) {
  const query = String(payload.query ?? "");
  const handler = handlers[query];
  if (!handler) {
    throw new Error(`${query} admin query handler not found`);
  }
  const data = await handler({
    query,
    request: undefined as unknown as IncomingMessage,
  });
  return { data: data ?? null };
}

async function proxyGrpcHttpRoute(
  payload: Record<string, unknown>,
  handlers: Record<string, ModuleHttpHandler>
) {
  const method = String(payload.method ?? "") as ModuleHttpMethod;
  const declaredPath = String(
    payload.declared_path ?? payload.remote_path ?? ""
  );
  const handler = handlers[routeKey(method, declaredPath)];
  if (!handler) {
    return {
      body: {
        error: {
          code: "not_found",
          message: `${method} ${declaredPath} handler not found`,
        },
      },
      status_code: 404,
    };
  }
  const result = normalizeHandlerResult(
    await handler({
      body: payload.body,
      params:
        typeof payload.path_params === "object" && payload.path_params !== null
          ? (payload.path_params as Record<string, string>)
          : {},
      request: undefined as unknown as IncomingMessage,
      url: new URL(String(payload.remote_path ?? "/"), "http://127.0.0.1"),
    })
  );
  return {
    body: result.body,
    status_code: result.statusCode,
  };
}

async function invokeGrpcRuntimeFunction(
  payload: Record<string, unknown>,
  handlers: Record<string, ModuleRuntimeHandler>
) {
  const functionName = String(payload.function_name ?? "");
  const handler = handlers[functionName];
  if (!handler) {
    throw new Error(`${functionName} runtime function handler not found`);
  }
  const output = await handler({
    input: payload.input,
    invocation: payload as unknown as ModuleRuntimeInvokeRequest,
    request: undefined as unknown as IncomingMessage,
  });
  return { output: output ?? null };
}
