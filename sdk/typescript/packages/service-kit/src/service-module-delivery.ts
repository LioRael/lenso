/* eslint-disable complexity, func-style, no-use-before-define */
import { createHash, randomUUID, timingSafeEqual } from "node:crypto";
import { once } from "node:events";
import { createServer, STATUS_CODES } from "node:http";
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
  actor?: ProviderActorContext;
  body: unknown;
  params: Record<string, string>;
  request: IncomingMessage;
  url: URL;
}

export interface ProblemErrorDetail {
  field: string | null;
  reason: string;
}

export interface ProblemDetails {
  type: string;
  title: string;
  status: number;
  detail: string;
  code: string;
  request_id: string | null;
  correlation_id: string | null;
  errors: readonly ProblemErrorDetail[];
  next_actions?: readonly string[];
}

export interface ProblemDetailsOptions {
  code: string;
  detail: string;
  status: number;
  request?: IncomingMessage;
  title?: string;
  type?: string;
  errors?: readonly ProblemErrorDetail[];
  nextActions?: readonly string[];
}

export type ModuleHttpHandlerResult =
  | unknown
  | {
      body: unknown;
      statusCode?: number;
    };

export const problemDetails = ({
  code,
  detail,
  status,
  request,
  title,
  type,
  errors = [],
  nextActions,
}: ProblemDetailsOptions): { body: ProblemDetails; statusCode: number } => {
  const context = request ? readLensoInvocationContext(request) : {};
  return {
    body: {
      code,
      correlation_id: context.correlationId ?? null,
      detail,
      errors,
      ...(nextActions ? { next_actions: nextActions } : {}),
      request_id: context.requestId ?? null,
      status,
      title: title ?? problemTitle(status),
      type: type ?? `https://lenso.dev/problems/${code}`,
    },
    statusCode: status,
  };
};

const problemTitle = (status: number): string => {
  switch (status) {
    case 400:
      return "Validation failed";
    case 401:
      return "Unauthorized";
    case 403:
      return "Forbidden";
    case 404:
      return "Not found";
    case 409:
      return "Conflict";
    case 429:
      return "Rate limited";
    case 500:
      return "Internal error";
    case 502:
      return "External dependency failure";
    default:
      return STATUS_CODES[status] ?? "HTTP error";
  }
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

export interface ServiceModuleRequirement {
  module_id: string;
  version_requirement: string;
  capabilities: readonly string[];
  optional: boolean;
}

export interface ServiceModuleConsoleSurface {
  name: string;
  label: string;
  route: string;
  presentation: {
    kind: "esm";
    entry: string;
  };
  required_capabilities: readonly string[];
  icon?: string;
  navigation?: {
    workspace: {
      id: string;
      label: string;
      icon?: string;
    };
    group?: {
      id: string;
      label: string;
      order?: number;
    };
    order?: number;
  };
}

export interface ServiceModuleManifest {
  protocol: "lenso.module-manifest.v1";
  module_id: string;
  story_display: readonly ModuleStoryDisplayDescriptor[];
  capabilities: readonly string[];
  requires: readonly ServiceModuleRequirement[];
  http_routes: readonly ModuleHttpRoute[];
  runtime: {
    functions: readonly ModuleRuntimeFunctionDeclaration[];
    schedules: readonly unknown[];
  };
  events?: ModuleEventSurface;
  lifecycle?: ModuleLifecycleSurface;
  admin: unknown | null;
  console: readonly ServiceModuleConsoleSurface[];
  console_slots: readonly unknown[];
  console_contributions: readonly unknown[];
  /** @deprecated Use module_id. This compatibility alias is omitted from JSON. */
  readonly name: string;
  /** @deprecated Module versions belong to Module Releases. This alias is omitted from JSON. */
  readonly version: string;
  /** @deprecated Use requires. This compatibility alias is omitted from JSON. */
  readonly dependencies: readonly string[];
}

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
  providerV1?: ProviderV1Options;
  status?: ServiceStatusOptions;
  onReady?: (server: ServedService) => void;
}

type ProviderAwareModuleAdminDataSource = {
  list: (query: {
    limit: number;
    cursor?: string;
  }) =>
    | ModuleAdminPage
    | ProviderV1HandlerOutcome
    | Promise<ModuleAdminPage | ProviderV1HandlerOutcome>;
  detail: (
    id: string
  ) =>
    | unknown
    | null
    | undefined
    | ProviderV1HandlerOutcome
    | Promise<unknown | null | undefined | ProviderV1HandlerOutcome>;
};

type ProviderAwareModuleEventHandler = (
  context: ModuleEventHandlerContext
) =>
  | ModuleEventHandleResponse
  | ProviderV1HandlerOutcome
  | undefined
  | Promise<
      ModuleEventHandleResponse | ProviderV1HandlerOutcome | undefined
    >;

type ProviderAwareServiceModuleHandlers = Omit<
  ServiceModuleHandlers,
  "data" | "events"
> & {
  data?: Record<string, ProviderAwareModuleAdminDataSource>;
  events?: Record<string, ProviderAwareModuleEventHandler>;
};

type ProviderAwareServeServiceOptions = Omit<ServeServiceOptions, "modules"> & {
  modules?: Record<string, ProviderAwareServiceModuleHandlers>;
};

export interface ProviderV1Export {
  exportKey: string;
  moduleId: string;
  moduleVersion: string;
  moduleReleaseDigest: string;
  manifestDigest: string;
  manifest: Record<string, unknown>;
  contractDigests: Record<string, string>;
  ready?: boolean;
  readinessReasons?: readonly string[];
}

export type ProviderActorContext =
  | { kind: "anonymous" }
  | { kind: "system" }
  | { kind: "user"; user_id: string; scopes: readonly string[] }
  | { kind: "service"; service_id: string; scopes: readonly string[] };

export interface ProviderV1Options {
  protocolContractDigest: string;
  serviceId: string;
  serviceReleaseVersion: string;
  serviceReleaseDigest: string;
  runtimeInstanceId: string;
  exports: readonly ProviderV1Export[];
  features?: readonly string[];
  invocationStore?: ProviderInvocationStore;
  moduleReleases?: Readonly<Record<string, Record<string, unknown>>>;
  /** Required when Provider V1 is exposed beyond loopback. Never advertised. */
  bearerToken?: string;
}

export type ProviderV1InvocationMode = "read_only" | "durable";

export interface ProviderV1Invocation {
  protocol: "lenso.provider.v1";
  invocationId: string;
  requestId: string;
  attempt: number;
  deadline: string;
  serviceReleaseDigest: string;
  exportKey: string;
  moduleReleaseDigest: string;
  manifestDigest: string;
  operationKind: string;
  operationName: string;
  operationVersion: string;
  mode: ProviderV1InvocationMode;
  inputContractDigest: string;
  outputContractDigest: string;
  tenantId?: string | null;
  actor: ProviderActorContext;
  delegation?: unknown;
  locale?: string | null;
  context?: Readonly<Record<string, unknown>>;
  correlationId: string;
  causationId?: string | null;
  trace: unknown;
  contentType: string;
  payload: unknown;
}

export type ProviderV1OutcomeStatus =
  | "pending"
  | "succeeded"
  | "rejected"
  | "failed";

export interface ProviderV1ErrorDetail {
  field: string | null;
  reason: string;
}

export interface ProviderV1Error {
  code: string;
  message: string;
  retryable: boolean;
  retryAfterMs: number | null;
  providerTraceReference: string | null;
  details: readonly ProviderV1ErrorDetail[];
}

export interface ProviderV1ErrorInput {
  code: string;
  message: string;
  retryable?: boolean;
  retryAfterMs?: number | null;
  providerTraceReference?: string | null;
  details?: readonly ProviderV1ErrorDetail[];
}

export interface ProviderV1HostEventEffect {
  eventId: string;
  eventName: string;
  eventVersion: number;
  sourceModule: string;
  aggregateType: string;
  aggregateId: string;
  correlationId: string;
  causationId?: string | null;
  occurredAt: string;
  payload: unknown;
  headers?: unknown;
}

export interface ProviderV1TraceContext {
  trace_id?: string | null;
  span_id?: string | null;
  baggage?: readonly (readonly [string, string])[];
}

export interface ProviderV1HostRuntimeFunctionRequest {
  requestId: string;
  functionName: string;
  input: unknown;
  correlationId: string;
  actor: ProviderActorContext;
  tenantId?: string | null;
  trace?: ProviderV1TraceContext;
  causationId?: string | null;
  maxAttempts?: number | null;
}

export interface ProviderV1HostEffects {
  events: readonly ProviderV1HostEventEffect[];
  runtimeFunctionRequests: readonly ProviderV1HostRuntimeFunctionRequest[];
}

export interface ProviderV1Outcome {
  protocol: "lenso.provider.v1";
  invocationId: string;
  status: ProviderV1OutcomeStatus;
  result: unknown;
  error: ProviderV1Error | null;
  effectEvidence: readonly unknown[];
  hostEffects: ProviderV1HostEffects;
  outcomeDigest: string;
}

const providerV1HandlerOutcomeMarker = Symbol(
  "@lenso/service-kit/provider-v1-handler-outcome"
);

export interface ProviderV1HandlerOutcome {
  readonly [providerV1HandlerOutcomeMarker]: true;
  status: ProviderV1OutcomeStatus;
  result: unknown;
  error: ProviderV1ErrorInput | null;
  effectEvidence: readonly unknown[];
  hostEffects: ProviderV1HostEffects;
}

export interface ProviderV1SucceededOptions {
  effectEvidence?: readonly unknown[];
  hostEffects?: Partial<ProviderV1HostEffects>;
}

export interface ProviderV1PendingOptions {
  error?: ProviderV1ErrorInput | null;
  effectEvidence?: readonly unknown[];
}

export interface ProviderV1FailureOptions {
  effectEvidence?: readonly unknown[];
}

export type ProviderInvocationStoreDurability = "process" | "durable";
export type ProviderStoredInvocationPhase =
  | "executing"
  | "pending"
  | "completed";

export interface ProviderStoredInvocation {
  invocationId: string;
  requestDigest: string;
  phase: ProviderStoredInvocationPhase;
  outcome: ProviderV1Outcome;
  createdAt: string;
  updatedAt: string;
  acknowledgedAt: string | null;
  acknowledgedOutcomeDigest: string | null;
}

export interface ProviderInvocationStoreClaimInput {
  invocationId: string;
  requestDigest: string;
  pendingOutcome: ProviderV1Outcome;
  now: string;
}

export type ProviderInvocationStoreClaimResult =
  | { kind: "claimed"; record: ProviderStoredInvocation }
  | { kind: "replay"; record: ProviderStoredInvocation }
  | { kind: "conflict" };

export interface ProviderInvocationStoreCompleteInput {
  invocationId: string;
  requestDigest: string;
  outcome: ProviderV1Outcome;
  now: string;
}

export interface ProviderInvocationStoreAcknowledgeInput {
  invocationId: string;
  outcomeDigest: string;
  now: string;
}

export type ProviderInvocationStoreAcknowledgeResult =
  | { kind: "acknowledged"; record: ProviderStoredInvocation }
  | { kind: "not_found" }
  | { kind: "conflict" };

/**
 * Durable implementations must make each method atomic across all service
 * instances. `claim` owns the unique invocation id, `complete` compares the
 * request digest before writing an immutable final outcome, and `acknowledge`
 * compares the exact outcome digest. Replacing an acknowledged executing or
 * pending outcome with a new outcome must clear the stale acknowledgement.
 * Methods must resolve only after their writes are durable.
 */
export interface ProviderInvocationStore {
  readonly durability: ProviderInvocationStoreDurability;
  claim: (
    input: ProviderInvocationStoreClaimInput
  ) => Promise<ProviderInvocationStoreClaimResult>;
  get: (invocationId: string) => Promise<ProviderStoredInvocation | undefined>;
  complete: (
    input: ProviderInvocationStoreCompleteInput
  ) => Promise<ProviderStoredInvocation>;
  acknowledge: (
    input: ProviderInvocationStoreAcknowledgeInput
  ) => Promise<ProviderInvocationStoreAcknowledgeResult>;
}

export interface ProviderInvocationStoreConformanceOptions {
  /**
   * Return a fresh adapter instance connected to the same durable backend.
   * The conformance vector calls this repeatedly to prove restart recovery.
   */
  createStore: () => ProviderInvocationStore | Promise<ProviderInvocationStore>;
  invocationId?: string;
}

export interface ProviderInvocationStoreConformanceResult {
  invocationId: string;
  outcomeDigest: string;
}

const providerV1Protocol = "lenso.provider.v1" as const;
const providerV1BasePath = "/lenso/provider/v1";

const canonicalJsonString = (value: string) => {
  for (let index = 0; index < value.length; index += 1) {
    const code = value.charCodeAt(index);
    if (code >= 0xd800 && code <= 0xdbff) {
      const next = value.charCodeAt(index + 1);
      if (!(next >= 0xdc00 && next <= 0xdfff)) {
        throw new Error("Canonical JSON cannot contain an unpaired surrogate");
      }
      index += 1;
    } else if (code >= 0xdc00 && code <= 0xdfff) {
      throw new Error("Canonical JSON cannot contain an unpaired surrogate");
    }
  }
  return JSON.stringify(value);
};

const canonicalJson = (
  value: unknown,
  seen: WeakSet<object> = new WeakSet()
): string => {
  if (value === null || typeof value === "boolean") {
    return JSON.stringify(value);
  }
  if (typeof value === "string") {
    return canonicalJsonString(value);
  }
  if (typeof value === "number") {
    if (!Number.isFinite(value)) {
      throw new Error("Canonical JSON cannot contain a non-finite number");
    }
    return JSON.stringify(value);
  }
  if (typeof value !== "object") {
    throw new Error(`Canonical JSON cannot contain a ${typeof value} value`);
  }
  if (seen.has(value)) {
    throw new Error("Canonical JSON cannot contain a cyclic value");
  }
  seen.add(value);
  try {
    if (Array.isArray(value)) {
      const entries: string[] = [];
      for (let index = 0; index < value.length; index += 1) {
        if (!Object.hasOwn(value, index)) {
          throw new Error("Canonical JSON cannot contain a sparse array");
        }
        entries.push(canonicalJson(value[index], seen));
      }
      return `[${entries.join(",")}]`;
    }
    const prototype = Object.getPrototypeOf(value) as unknown;
    if (prototype !== Object.prototype && prototype !== null) {
      throw new Error("Canonical JSON cannot contain a non-JSON object");
    }
    if (Object.getOwnPropertySymbols(value).length > 0) {
      throw new Error("Canonical JSON cannot contain symbol-keyed values");
    }
    return `{${Object.keys(value)
      .sort()
      .map(
        (key) =>
          `${canonicalJsonString(key)}:${canonicalJson(
            (value as Record<string, unknown>)[key],
            seen
          )}`
      )
      .join(",")}}`;
  } finally {
    seen.delete(value);
  }
};

export const providerV1OutcomeLimits = Object.freeze({
  maxEffectEvidenceItems: 100,
  maxErrorDetails: 100,
  maxErrorMessageBytes: 4_096,
  maxHostEffects: 100,
  maxOutcomeBytes: 1024 * 1024,
  maxProviderTraceReferenceBytes: 512,
  maxRetryAfterMs: 24 * 60 * 60 * 1000,
});

const emptyProviderHostEffects = (): ProviderV1HostEffects => ({
  events: [],
  runtimeFunctionRequests: [],
});

export const providerSucceeded = (
  result: unknown,
  options: ProviderV1SucceededOptions = {}
): ProviderV1HandlerOutcome => ({
  [providerV1HandlerOutcomeMarker]: true,
  effectEvidence: options.effectEvidence ?? [],
  error: null,
  hostEffects: {
    events: options.hostEffects?.events ?? [],
    runtimeFunctionRequests:
      options.hostEffects?.runtimeFunctionRequests ?? [],
  },
  result,
  status: "succeeded",
});

export const providerPending = (
  options: ProviderV1PendingOptions = {}
): ProviderV1HandlerOutcome => ({
  [providerV1HandlerOutcomeMarker]: true,
  effectEvidence: options.effectEvidence ?? [],
  error: options.error ?? null,
  hostEffects: emptyProviderHostEffects(),
  result: null,
  status: "pending",
});

export const providerRejected = (
  error: ProviderV1ErrorInput,
  options: ProviderV1FailureOptions = {}
): ProviderV1HandlerOutcome => ({
  [providerV1HandlerOutcomeMarker]: true,
  effectEvidence: options.effectEvidence ?? [],
  error,
  hostEffects: emptyProviderHostEffects(),
  result: null,
  status: "rejected",
});

export const providerFailed = (
  error: ProviderV1ErrorInput,
  options: ProviderV1FailureOptions = {}
): ProviderV1HandlerOutcome => ({
  [providerV1HandlerOutcomeMarker]: true,
  effectEvidence: options.effectEvidence ?? [],
  error,
  hostEffects: emptyProviderHostEffects(),
  result: null,
  status: "failed",
});

const isProviderHandlerOutcome = (
  value: unknown
): value is ProviderV1HandlerOutcome =>
  typeof value === "object" &&
  value !== null &&
  providerV1HandlerOutcomeMarker in value;

const mapProviderHandlerOutcome = (
  outcome: ProviderV1HandlerOutcome,
  mapResult: (result: unknown) => unknown
): ProviderV1HandlerOutcome =>
  outcome.status === "succeeded"
    ? { ...outcome, result: mapResult(outcome.result) }
    : outcome;

const normalizeJsonValue = (
  value: unknown,
  seen: WeakSet<object> = new WeakSet()
): unknown => {
  if (
    value === null ||
    typeof value === "boolean"
  ) {
    return value;
  }
  if (typeof value === "string") {
    canonicalJsonString(value);
    return value;
  }
  if (typeof value === "number") {
    if (!Number.isFinite(value)) {
      throw new Error("Provider outcome contains a non-finite number");
    }
    return value;
  }
  if (typeof value !== "object") {
    throw new Error(`Provider outcome contains a ${typeof value} value`);
  }
  if (seen.has(value)) {
    throw new Error("Provider outcome contains a cyclic value");
  }
  seen.add(value);
  try {
    if (Array.isArray(value)) {
      const entries: unknown[] = [];
      for (let index = 0; index < value.length; index += 1) {
        if (!Object.hasOwn(value, index)) {
          throw new Error("Provider outcome contains a sparse array");
        }
        entries.push(normalizeJsonValue(value[index], seen));
      }
      return entries;
    }
    const prototype = Object.getPrototypeOf(value) as unknown;
    if (prototype !== Object.prototype && prototype !== null) {
      throw new Error("Provider outcome contains a non-JSON object");
    }
    if (Object.getOwnPropertySymbols(value).length > 0) {
      throw new Error("Provider outcome contains a symbol-keyed value");
    }
    return Object.fromEntries(
      Object.entries(value as Record<string, unknown>)
        .sort(([left], [right]) =>
          left === right ? 0 : left < right ? -1 : 1
        )
        .map(([key, entry]) => {
          canonicalJsonString(key);
          return [key, normalizeJsonValue(entry, seen)];
        })
    );
  } finally {
    seen.delete(value);
  }
};

const normalizeProviderError = (
  value: ProviderV1ErrorInput
): ProviderV1Error => {
  if (
    typeof value.code !== "string" ||
    !value.code.trim() ||
    typeof value.message !== "string" ||
    !value.message.trim()
  ) {
    throw new Error("Provider outcome errors require code and message");
  }
  if (!/^[a-z0-9][a-z0-9._-]{0,127}$/u.test(value.code)) {
    throw new Error("Provider outcome error code is invalid");
  }
  if (
    /[\u0000-\u001f\u007f]/u.test(value.message) ||
    Buffer.byteLength(value.message, "utf8") >
      providerV1OutcomeLimits.maxErrorMessageBytes
  ) {
    throw new Error(
      "Provider outcome error message must be control-free and at most " +
        `${providerV1OutcomeLimits.maxErrorMessageBytes} bytes`
    );
  }
  if (
    value.retryable !== undefined &&
    typeof value.retryable !== "boolean"
  ) {
    throw new Error("Provider outcome retryable must be a boolean");
  }
  if (
    value.retryAfterMs !== undefined &&
    value.retryAfterMs !== null &&
    (!Number.isSafeInteger(value.retryAfterMs) || value.retryAfterMs < 0)
  ) {
    throw new Error("Provider outcome retryAfterMs must be a non-negative integer");
  }
  if (
    value.retryAfterMs !== undefined &&
    value.retryAfterMs !== null &&
    value.retryAfterMs > providerV1OutcomeLimits.maxRetryAfterMs
  ) {
    throw new Error(
      `Provider outcome retryAfterMs must not exceed ${providerV1OutcomeLimits.maxRetryAfterMs}`
    );
  }
  if (
    value.retryAfterMs !== undefined &&
    value.retryAfterMs !== null &&
    value.retryable !== true
  ) {
    throw new Error("Provider outcome retryAfterMs requires retryable: true");
  }
  if (
    value.providerTraceReference !== undefined &&
    value.providerTraceReference !== null &&
    (typeof value.providerTraceReference !== "string" ||
      !value.providerTraceReference.trim() ||
      /[\u0000-\u001f\u007f]/u.test(value.providerTraceReference) ||
      Buffer.byteLength(value.providerTraceReference, "utf8") >
        providerV1OutcomeLimits.maxProviderTraceReferenceBytes)
  ) {
    throw new Error(
      "Provider outcome providerTraceReference must be non-empty, " +
        `control-free, and at most ${providerV1OutcomeLimits.maxProviderTraceReferenceBytes} bytes`
    );
  }
  if (
    value.details !== undefined &&
    (!Array.isArray(value.details) ||
      value.details.length > providerV1OutcomeLimits.maxErrorDetails)
  ) {
    throw new Error(
      `Provider outcome errors allow at most ${providerV1OutcomeLimits.maxErrorDetails} details`
    );
  }
  const details = (value.details ?? []).map((detail) => {
    if (
      (detail.field !== null && typeof detail.field !== "string") ||
      typeof detail.reason !== "string" ||
      !detail.reason.trim()
    ) {
      throw new Error("Provider outcome error details require a reason");
    }
    return { field: detail.field, reason: detail.reason };
  });
  return {
    code: value.code,
    message: value.message,
    retryable: value.retryable ?? false,
    retryAfterMs: value.retryAfterMs ?? null,
    providerTraceReference: value.providerTraceReference ?? null,
    details,
  };
};

const normalizeProviderActor = (
  actor: ProviderActorContext
): ProviderActorContext => {
  if (!validProviderActor(actor)) {
    throw new Error("Provider Host Runtime effect actor is invalid");
  }
  switch (actor.kind) {
    case "user":
      return { kind: "user", user_id: actor.user_id, scopes: [...actor.scopes] };
    case "service":
      return {
        kind: "service",
        service_id: actor.service_id,
        scopes: [...actor.scopes],
      };
    case "system":
      return { kind: "system" };
    default:
      return { kind: "anonymous" };
  }
};

const normalizeProviderTrace = (
  trace: ProviderV1TraceContext | undefined
): Required<ProviderV1TraceContext> => {
  if (
    trace?.trace_id !== undefined &&
    trace.trace_id !== null &&
    typeof trace.trace_id !== "string"
  ) {
    throw new Error("Provider trace_id must be a string or null");
  }
  if (
    trace?.span_id !== undefined &&
    trace.span_id !== null &&
    typeof trace.span_id !== "string"
  ) {
    throw new Error("Provider span_id must be a string or null");
  }
  const baggage = (trace?.baggage ?? []).map((entry) => {
    if (
      !Array.isArray(entry) ||
      entry.length !== 2 ||
      typeof entry[0] !== "string" ||
      typeof entry[1] !== "string"
    ) {
      throw new Error("Provider Host Runtime effect trace baggage is invalid");
    }
    return [entry[0], entry[1]] as const;
  });
  return {
    trace_id: trace?.trace_id ?? null,
    span_id: trace?.span_id ?? null,
    baggage,
  };
};

const requireProviderEffectText = (value: string, field: string) => {
  if (typeof value !== "string" || !value.trim()) {
    throw new Error(`Provider Host effect ${field} must be non-empty`);
  }
  return value;
};

const normalizeProviderTimestamp = (value: string) => {
  if (
    typeof value !== "string" ||
    !/T.*(?:Z|[+-]\d{2}:\d{2})$/u.test(value) ||
    Number.isNaN(Date.parse(value))
  ) {
    throw new Error("Provider Host Event effect occurredAt is invalid");
  }
  const fractional = value.match(/\.(\d+)(?:Z|[+-]\d{2}:\d{2})$/u)?.[1];
  if (fractional && fractional.slice(3).replace(/0/gu, "") !== "") {
    throw new Error(
      "Provider Host Event effect occurredAt exceeds millisecond precision"
    );
  }
  return new Date(value).toISOString().replace(/\.000Z$/u, "Z");
};

const normalizeHostEffects = (
  invocation: ProviderV1Invocation,
  effects: ProviderV1HostEffects,
  providerExport: ProviderV1Export
): ProviderV1HostEffects => {
  const count = effects.events.length + effects.runtimeFunctionRequests.length;
  if (count > providerV1OutcomeLimits.maxHostEffects) {
    throw new Error(
      `Provider outcome exceeds ${providerV1OutcomeLimits.maxHostEffects} Host effects`
    );
  }
  const correlationId = invocation.correlationId;
  const eventIds = new Set<string>();
  const events = effects.events.map((event) => {
    if (
      !Number.isInteger(event.eventVersion) ||
      event.eventVersion < 0 ||
      event.eventVersion > 65_535
    ) {
      throw new Error("Provider Host Event effect version is invalid");
    }
    if (correlationId !== undefined && event.correlationId !== correlationId) {
      throw new Error(
        "Provider Host Event effect correlationId does not match the invocation"
      );
    }
    if (event.sourceModule !== providerExport.moduleId) {
      throw new Error(
        "Provider Host Event effect sourceModule does not match the locked Module"
      );
    }
    if (eventIds.has(event.eventId)) {
      throw new Error("Provider Host Event effect eventId is duplicated");
    }
    eventIds.add(event.eventId);
    return {
      eventId: requireProviderEffectText(event.eventId, "eventId"),
      eventName: requireProviderEffectText(event.eventName, "eventName"),
      eventVersion: event.eventVersion,
      sourceModule: requireProviderEffectText(event.sourceModule, "sourceModule"),
      aggregateType: requireProviderEffectText(
        event.aggregateType,
        "aggregateType"
      ),
      aggregateId: requireProviderEffectText(event.aggregateId, "aggregateId"),
      correlationId: requireProviderEffectText(
        event.correlationId,
        "correlationId"
      ),
      causationId: event.causationId ?? null,
      occurredAt: normalizeProviderTimestamp(event.occurredAt),
      payload: normalizeJsonValue(event.payload),
      headers: normalizeJsonValue(event.headers ?? {}),
    };
  });
  const manifestRuntime = providerExport.manifest.runtime as
    | { functions?: readonly { name?: unknown }[] }
    | undefined;
  const allowedRuntimeFunctions = new Set(
    (manifestRuntime?.functions ?? [])
      .map((definition) => definition.name)
      .filter((name): name is string => typeof name === "string")
  );
  const runtimeRequestIds = new Set<string>();
  const runtimeFunctionRequests = effects.runtimeFunctionRequests.map(
    (runtimeRequest) => {
      if (
        runtimeRequest.maxAttempts !== undefined &&
        runtimeRequest.maxAttempts !== null &&
        (!Number.isInteger(runtimeRequest.maxAttempts) ||
          runtimeRequest.maxAttempts < 1 ||
          runtimeRequest.maxAttempts > 100)
      ) {
        throw new Error(
          "Provider Host Runtime effect maxAttempts must be between 1 and 100"
        );
      }
      if (
        correlationId !== undefined &&
        runtimeRequest.correlationId !== correlationId
      ) {
        throw new Error(
          "Provider Host Runtime effect correlationId does not match the invocation"
        );
      }
      if (!allowedRuntimeFunctions.has(runtimeRequest.functionName)) {
        throw new Error(
          "Provider Host Runtime effect functionName is not declared by the locked Module"
        );
      }
      const actor = normalizeProviderActor(runtimeRequest.actor);
      const tenantId = runtimeRequest.tenantId ?? null;
      const trace = normalizeProviderTrace(runtimeRequest.trace);
      if (canonicalJson(actor) !== canonicalJson(invocation.actor)) {
        throw new Error(
          "Provider Host Runtime effect actor does not match the invocation"
        );
      }
      if (tenantId !== (invocation.tenantId ?? null)) {
        throw new Error(
          "Provider Host Runtime effect tenantId does not match the invocation"
        );
      }
      if (canonicalJson(trace) !== canonicalJson(invocation.trace)) {
        throw new Error(
          "Provider Host Runtime effect trace does not match the invocation"
        );
      }
      if (runtimeRequestIds.has(runtimeRequest.requestId)) {
        throw new Error(
          "Provider Host Runtime effect requestId is duplicated"
        );
      }
      runtimeRequestIds.add(runtimeRequest.requestId);
      return {
        requestId: requireProviderEffectText(
          runtimeRequest.requestId,
          "requestId"
        ),
        functionName: requireProviderEffectText(
          runtimeRequest.functionName,
          "functionName"
        ),
        input: normalizeJsonValue(runtimeRequest.input),
        correlationId: requireProviderEffectText(
          runtimeRequest.correlationId,
          "correlationId"
        ),
        actor,
        tenantId,
        trace,
        causationId: runtimeRequest.causationId ?? null,
        maxAttempts: runtimeRequest.maxAttempts ?? null,
      };
    }
  );
  return { events, runtimeFunctionRequests };
};

const outcomeDigest = (outcome: Omit<ProviderV1Outcome, "outcomeDigest">) =>
  canonicalDigest({ ...outcome, outcomeDigest: "" });

const providerOutcome = (
  invocation: ProviderV1Invocation,
  handlerResult: unknown,
  providerExport: ProviderV1Export
): ProviderV1Outcome => {
  const handlerOutcome = isProviderHandlerOutcome(handlerResult)
    ? handlerResult
    : providerSucceeded(handlerResult);
  const error = handlerOutcome.error
    ? normalizeProviderError(handlerOutcome.error)
    : null;
  if (
    (handlerOutcome.status === "failed" ||
      handlerOutcome.status === "rejected") &&
    error === null
  ) {
    throw new Error(
      `Provider ${handlerOutcome.status} outcomes require an error`
    );
  }
  if (handlerOutcome.status === "rejected" && error?.retryable) {
    throw new Error("Provider rejected outcomes cannot be retryable");
  }
  if (
    handlerOutcome.status !== "succeeded" &&
    (handlerOutcome.hostEffects.events.length > 0 ||
      handlerOutcome.hostEffects.runtimeFunctionRequests.length > 0)
  ) {
    throw new Error("Only succeeded Provider outcomes may contain Host effects");
  }
  if (
    handlerOutcome.effectEvidence.length >
    providerV1OutcomeLimits.maxEffectEvidenceItems
  ) {
    throw new Error(
      "Provider outcome exceeds " +
        `${providerV1OutcomeLimits.maxEffectEvidenceItems} effect evidence items`
    );
  }
  const withoutDigest: Omit<ProviderV1Outcome, "outcomeDigest"> = {
    protocol: providerV1Protocol,
    invocationId: invocation.invocationId,
    status: handlerOutcome.status,
    result: normalizeJsonValue(handlerOutcome.result),
    error,
    effectEvidence: handlerOutcome.effectEvidence.map((item) =>
      normalizeJsonValue(item)
    ),
    hostEffects: normalizeHostEffects(
      invocation,
      handlerOutcome.hostEffects,
      providerExport
    ),
  };
  const outcome: ProviderV1Outcome = {
    ...withoutDigest,
    outcomeDigest: outcomeDigest(withoutDigest),
  };
  if (
    Buffer.byteLength(JSON.stringify(outcome), "utf8") >
    providerV1OutcomeLimits.maxOutcomeBytes
  ) {
    throw new Error(
      `Provider outcome exceeds ${providerV1OutcomeLimits.maxOutcomeBytes} bytes`
    );
  }
  return outcome;
};

const cloneStoredInvocation = (
  value: ProviderStoredInvocation
): ProviderStoredInvocation => structuredClone(value);

class MemoryProviderInvocationStore implements ProviderInvocationStore {
  readonly durability = "process" as const;
  readonly #records = new Map<string, ProviderStoredInvocation>();

  async claim(
    input: ProviderInvocationStoreClaimInput
  ): Promise<ProviderInvocationStoreClaimResult> {
    const existing = this.#records.get(input.invocationId);
    if (existing) {
      if (existing.requestDigest !== input.requestDigest) {
        return { kind: "conflict" };
      }
      return { kind: "replay", record: cloneStoredInvocation(existing) };
    }
    const record: ProviderStoredInvocation = {
      invocationId: input.invocationId,
      requestDigest: input.requestDigest,
      phase: "executing",
      outcome: structuredClone(input.pendingOutcome),
      createdAt: input.now,
      updatedAt: input.now,
      acknowledgedAt: null,
      acknowledgedOutcomeDigest: null,
    };
    this.#records.set(input.invocationId, record);
    return { kind: "claimed", record: cloneStoredInvocation(record) };
  }

  async get(
    invocationId: string
  ): Promise<ProviderStoredInvocation | undefined> {
    const record = this.#records.get(invocationId);
    return record ? cloneStoredInvocation(record) : undefined;
  }

  async complete(
    input: ProviderInvocationStoreCompleteInput
  ): Promise<ProviderStoredInvocation> {
    const existing = this.#records.get(input.invocationId);
    if (!existing || existing.requestDigest !== input.requestDigest) {
      throw new Error("Provider invocation completion conflicts with its claim");
    }
    if (existing.phase === "completed") {
      if (existing.outcome.outcomeDigest !== input.outcome.outcomeDigest) {
        throw new Error("Provider invocation final outcome is immutable");
      }
      return cloneStoredInvocation(existing);
    }
    if (
      existing.phase === "pending" &&
      input.outcome.status === "pending" &&
      existing.outcome.outcomeDigest !== input.outcome.outcomeDigest
    ) {
      throw new Error("Provider invocation pending outcome cannot be rebound");
    }
    const outcomeChanged =
      existing.outcome.outcomeDigest !== input.outcome.outcomeDigest;
    const record: ProviderStoredInvocation = {
      ...existing,
      phase: input.outcome.status === "pending" ? "pending" : "completed",
      outcome: structuredClone(input.outcome),
      updatedAt: input.now,
      acknowledgedAt: outcomeChanged ? null : existing.acknowledgedAt,
      acknowledgedOutcomeDigest: outcomeChanged
        ? null
        : existing.acknowledgedOutcomeDigest,
    };
    this.#records.set(input.invocationId, record);
    return cloneStoredInvocation(record);
  }

  async acknowledge(
    input: ProviderInvocationStoreAcknowledgeInput
  ): Promise<ProviderInvocationStoreAcknowledgeResult> {
    const existing = this.#records.get(input.invocationId);
    if (!existing) {
      return { kind: "not_found" };
    }
    if (existing.outcome.outcomeDigest !== input.outcomeDigest) {
      return { kind: "conflict" };
    }
    if (existing.acknowledgedOutcomeDigest === input.outcomeDigest) {
      return { kind: "acknowledged", record: cloneStoredInvocation(existing) };
    }
    const record: ProviderStoredInvocation = {
      ...existing,
      updatedAt: input.now,
      acknowledgedAt: input.now,
      acknowledgedOutcomeDigest: input.outcomeDigest,
    };
    this.#records.set(input.invocationId, record);
    return { kind: "acknowledged", record: cloneStoredInvocation(record) };
  }
}

/** Process-local store for development and tests; it never advertises durability. */
export const createMemoryProviderInvocationStore =
  (): ProviderInvocationStore => new MemoryProviderInvocationStore();

/**
 * Mutating conformance vector for durable Provider invocation Store adapters.
 * Run it against an isolated test database; it leaves a small set of rows so
 * restart, conflict, and retention behavior remain inspectable.
 */
export const verifyProviderInvocationStoreConformance = async ({
  createStore,
  invocationId = `provider-store-conformance:${randomUUID()}`,
}: ProviderInvocationStoreConformanceOptions): Promise<
  ProviderInvocationStoreConformanceResult
> => {
  const adapters = new Set<ProviderInvocationStore>();
  const createDurableAdapter = async () => {
    const adapter = await createStore();
    if (adapter.durability !== "durable") {
      throw new Error(
        "Provider invocation Store conformance requires durable storage"
      );
    }
    if (adapters.has(adapter)) {
      throw new Error(
        "Provider invocation Store conformance requires fresh adapter instances"
      );
    }
    adapters.add(adapter);
    return adapter;
  };
  const requestDigest = canonicalDigest({ invocationId, vector: 1 });
  const conflictingDigest = canonicalDigest({ invocationId, vector: 2 });
  const claimPending = conformanceOutcome({
    invocationId,
    status: "pending",
  });
  const pending = conformanceOutcome({
    effectEvidence: [
      { kind: "remote_receipt", receiptId: "conformance-pending" },
    ],
    error: {
      code: "conformance_pending",
      details: [{ field: "receipt", reason: "delivery is not final" }],
      message: "The conformance delivery remains pending",
      providerTraceReference: "conformance-pending-trace",
      retryAfterMs: 750,
      retryable: true,
    },
    invocationId,
    status: "pending",
  });
  const failed = conformanceOutcome({
    effectEvidence: [
      { kind: "remote_receipt", receiptId: "conformance-failed" },
    ],
    error: {
      code: "conformance_failed",
      details: [{ field: null, reason: "the upstream remained unavailable" }],
      message: "The conformance delivery failed",
      providerTraceReference: "conformance-failed-trace",
      retryAfterMs: 2_500,
      retryable: true,
    },
    invocationId,
    status: "failed",
  });
  const [leftAdapter, rightAdapter] = await Promise.all([
    createDurableAdapter(),
    createDurableAdapter(),
  ]);
  const [left, right] = await Promise.all([
    leftAdapter.claim({
      invocationId,
      now: "2026-01-01T00:00:00.000Z",
      pendingOutcome: claimPending,
      requestDigest,
    }),
    rightAdapter.claim({
      invocationId,
      now: "2026-01-01T00:00:00.000Z",
      pendingOutcome: claimPending,
      requestDigest,
    }),
  ]);
  const kinds = [left.kind, right.kind].sort();
  if (kinds[0] !== "claimed" || kinds[1] !== "replay") {
    throw new Error("Provider invocation Store claim is not atomic");
  }
  for (const claim of [left, right]) {
    if (claim.kind === "conflict") {
      throw new Error("Provider invocation Store lost its concurrent claim");
    }
    assertConformanceRecord(
      claim.record,
      invocationId,
      requestDigest,
      "executing",
      claimPending
    );
  }
  const conflict = await (await createDurableAdapter()).claim({
    invocationId,
    now: "2026-01-01T00:00:01.000Z",
    pendingOutcome: claimPending,
    requestDigest: conflictingDigest,
  });
  if (conflict.kind !== "conflict") {
    throw new Error("Provider invocation Store allowed request identity rebinding");
  }
  const restarted = await createDurableAdapter();
  const recovered = await restarted.get(invocationId);
  assertConformanceRecord(
    recovered,
    invocationId,
    requestDigest,
    "executing",
    claimPending
  );
  let completionConflict = false;
  try {
    await restarted.complete({
      invocationId,
      now: "2026-01-01T00:00:01.500Z",
      outcome: failed,
      requestDigest: conflictingDigest,
    });
  } catch {
    completionConflict = true;
  }
  if (!completionConflict) {
    throw new Error(
      "Provider invocation Store completed a mismatched request digest"
    );
  }
  assertConformanceRecord(
    await (await createDurableAdapter()).get(invocationId),
    invocationId,
    requestDigest,
    "executing",
    claimPending
  );
  const pendingCompleted = await (await createDurableAdapter()).complete({
    invocationId,
    now: "2026-01-01T00:00:02.000Z",
    outcome: pending,
    requestDigest,
  });
  assertConformanceRecord(
    pendingCompleted,
    invocationId,
    requestDigest,
    "pending",
    pending
  );
  assertConformanceRecord(
    await (await createDurableAdapter()).get(invocationId),
    invocationId,
    requestDigest,
    "pending",
    pending
  );

  const [pendingAckLeftAdapter, pendingAckRightAdapter] = await Promise.all([
    createDurableAdapter(),
    createDurableAdapter(),
  ]);
  const pendingAcknowledgements = await Promise.all([
    pendingAckLeftAdapter.acknowledge({
      invocationId,
      now: "2026-01-01T00:00:03.000Z",
      outcomeDigest: pending.outcomeDigest,
    }),
    pendingAckRightAdapter.acknowledge({
      invocationId,
      now: "2026-01-01T00:00:03.500Z",
      outcomeDigest: pending.outcomeDigest,
    }),
  ]);
  const pendingAckRecords = pendingAcknowledgements.map((acknowledgement) => {
    if (acknowledgement.kind !== "acknowledged") {
      throw new Error(
        "Provider invocation Store did not atomically acknowledge a pending outcome"
      );
    }
    return assertConformanceRecord(
      acknowledgement.record,
      invocationId,
      requestDigest,
      "pending",
      pending
    );
  });
  if (
    pendingAckRecords[0]?.acknowledgedAt === null ||
    pendingAckRecords[0]?.acknowledgedAt !== pendingAckRecords[1]?.acknowledgedAt
  ) {
    throw new Error(
      "Provider invocation Store pending acknowledgement is not idempotent"
    );
  }

  const completed = await (await createDurableAdapter()).complete({
    invocationId,
    now: "2026-01-01T00:00:04.000Z",
    outcome: failed,
    requestDigest,
  });
  const completedRecord = assertConformanceRecord(
    completed,
    invocationId,
    requestDigest,
    "completed",
    failed
  );
  if (
    completedRecord.acknowledgedAt !== null ||
    completedRecord.acknowledgedOutcomeDigest !== null
  ) {
    throw new Error(
      "Provider invocation Store retained a stale pending acknowledgement"
    );
  }
  assertConformanceRecord(
    await (await createDurableAdapter()).get(invocationId),
    invocationId,
    requestDigest,
    "completed",
    failed
  );

  const differentOutcome = conformanceOutcome({
    invocationId,
    result: { conformance: false },
    status: "succeeded",
  });
  let immutable = false;
  try {
    await (await createDurableAdapter()).complete({
      invocationId,
      now: "2026-01-01T00:00:05.000Z",
      outcome: differentOutcome,
      requestDigest,
    });
  } catch {
    immutable = true;
  }
  if (!immutable) {
    throw new Error("Provider invocation Store replaced an immutable final outcome");
  }
  const wrongAck = await (await createDurableAdapter()).acknowledge({
    invocationId,
    now: "2026-01-01T00:00:06.000Z",
    outcomeDigest: conflictingDigest,
  });
  if (wrongAck.kind !== "conflict") {
    throw new Error("Provider invocation Store acknowledged the wrong outcome digest");
  }
  const afterWrongAck = assertConformanceRecord(
    await (await createDurableAdapter()).get(invocationId),
    invocationId,
    requestDigest,
    "completed",
    failed
  );
  if (
    afterWrongAck.acknowledgedAt !== null ||
    afterWrongAck.acknowledgedOutcomeDigest !== null
  ) {
    throw new Error("Provider invocation Store mutated a conflicting acknowledgement");
  }

  const [ackLeftAdapter, ackRightAdapter] = await Promise.all([
    createDurableAdapter(),
    createDurableAdapter(),
  ]);
  const [firstAck, replayedAck] = await Promise.all([
    ackLeftAdapter.acknowledge({
      invocationId,
      now: "2026-01-01T00:00:07.000Z",
      outcomeDigest: failed.outcomeDigest,
    }),
    ackRightAdapter.acknowledge({
      invocationId,
      now: "2026-01-01T00:00:08.000Z",
      outcomeDigest: failed.outcomeDigest,
    }),
  ]);
  if (
    firstAck.kind !== "acknowledged" ||
    replayedAck.kind !== "acknowledged" ||
    firstAck.record.acknowledgedAt === null ||
    firstAck.record.acknowledgedAt !== replayedAck.record.acknowledgedAt
  ) {
    throw new Error("Provider invocation Store acknowledgement is not idempotent");
  }
  assertConformanceRecord(
    firstAck.record,
    invocationId,
    requestDigest,
    "completed",
    failed
  );
  assertConformanceRecord(
    replayedAck.record,
    invocationId,
    requestDigest,
    "completed",
    failed
  );

  const rejectedInvocationId = `${invocationId}:rejected`;
  const rejectedRequestDigest = canonicalDigest({
    invocationId: rejectedInvocationId,
    vector: "rejected",
  });
  const rejectedClaim = conformanceOutcome({
    invocationId: rejectedInvocationId,
    status: "pending",
  });
  const rejected = conformanceOutcome({
    effectEvidence: [
      { kind: "remote_receipt", receiptId: "conformance-rejected" },
    ],
    error: {
      code: "conformance_rejected",
      details: [{ field: "recipient", reason: "recipient was rejected" }],
      message: "The conformance delivery was rejected",
      providerTraceReference: "conformance-rejected-trace",
      retryAfterMs: null,
      retryable: false,
    },
    invocationId: rejectedInvocationId,
    status: "rejected",
  });
  const rejectedClaimResult = await (await createDurableAdapter()).claim({
    invocationId: rejectedInvocationId,
    now: "2026-01-01T00:00:09.000Z",
    pendingOutcome: rejectedClaim,
    requestDigest: rejectedRequestDigest,
  });
  if (rejectedClaimResult.kind !== "claimed") {
    throw new Error("Provider invocation Store could not claim the rejected vector");
  }
  assertConformanceRecord(
    await (await createDurableAdapter()).complete({
      invocationId: rejectedInvocationId,
      now: "2026-01-01T00:00:10.000Z",
      outcome: rejected,
      requestDigest: rejectedRequestDigest,
    }),
    rejectedInvocationId,
    rejectedRequestDigest,
    "completed",
    rejected
  );
  assertConformanceRecord(
    await (await createDurableAdapter()).get(rejectedInvocationId),
    rejectedInvocationId,
    rejectedRequestDigest,
    "completed",
    rejected
  );

  const completionRaceInvocationId = `${invocationId}:completion-race`;
  const completionRaceRequestDigest = canonicalDigest({
    invocationId: completionRaceInvocationId,
    vector: "completion-race",
  });
  const completionRaceClaim = conformanceOutcome({
    invocationId: completionRaceInvocationId,
    status: "pending",
  });
  const completionRaceClaimResult = await (
    await createDurableAdapter()
  ).claim({
    invocationId: completionRaceInvocationId,
    now: "2026-01-01T00:00:11.000Z",
    pendingOutcome: completionRaceClaim,
    requestDigest: completionRaceRequestDigest,
  });
  if (completionRaceClaimResult.kind !== "claimed") {
    throw new Error("Provider invocation Store could not claim the completion race");
  }
  const completionRaceOutcomes = [
    conformanceOutcome({
      invocationId: completionRaceInvocationId,
      result: { winner: "left" },
      status: "succeeded",
    }),
    conformanceOutcome({
      invocationId: completionRaceInvocationId,
      result: { winner: "right" },
      status: "succeeded",
    }),
  ] as const;
  const [completionLeftAdapter, completionRightAdapter] = await Promise.all([
    createDurableAdapter(),
    createDurableAdapter(),
  ]);
  const completionResults = await Promise.allSettled([
    completionLeftAdapter.complete({
      invocationId: completionRaceInvocationId,
      now: "2026-01-01T00:00:12.000Z",
      outcome: completionRaceOutcomes[0],
      requestDigest: completionRaceRequestDigest,
    }),
    completionRightAdapter.complete({
      invocationId: completionRaceInvocationId,
      now: "2026-01-01T00:00:12.000Z",
      outcome: completionRaceOutcomes[1],
      requestDigest: completionRaceRequestDigest,
    }),
  ]);
  const completedRaceResults = completionResults.filter(
    (result): result is PromiseFulfilledResult<ProviderStoredInvocation> =>
      result.status === "fulfilled"
  );
  const rejectedRaceResults = completionResults.filter(
    (result) => result.status === "rejected"
  );
  if (completedRaceResults.length !== 1 || rejectedRaceResults.length !== 1) {
    throw new Error("Provider invocation Store final completion is not atomic");
  }
  const winningOutcome = completionRaceOutcomes.find(
    (outcome) =>
      outcome.outcomeDigest ===
      completedRaceResults[0]?.value.outcome.outcomeDigest
  );
  if (!winningOutcome) {
    throw new Error("Provider invocation Store returned an unknown race outcome");
  }
  assertConformanceRecord(
    completedRaceResults[0]?.value,
    completionRaceInvocationId,
    completionRaceRequestDigest,
    "completed",
    winningOutcome
  );
  assertConformanceRecord(
    await (await createDurableAdapter()).get(completionRaceInvocationId),
    completionRaceInvocationId,
    completionRaceRequestDigest,
    "completed",
    winningOutcome
  );

  const finalAdapter = await createDurableAdapter();
  const finalRecord = await finalAdapter.get(invocationId);
  if (
    finalRecord?.acknowledgedOutcomeDigest !== failed.outcomeDigest ||
    finalRecord.outcome.outcomeDigest !== failed.outcomeDigest
  ) {
    throw new Error("Provider invocation Store lost its acknowledgement after restart");
  }
  assertConformanceRecord(
    finalRecord,
    invocationId,
    requestDigest,
    "completed",
    failed
  );
  return { invocationId, outcomeDigest: failed.outcomeDigest };
};

const conformanceOutcome = ({
  effectEvidence = [],
  error = null,
  invocationId,
  result = null,
  status,
}: {
  effectEvidence?: readonly unknown[];
  error?: ProviderV1Error | null;
  invocationId: string;
  result?: unknown;
  status: ProviderV1OutcomeStatus;
}): ProviderV1Outcome => {
  const withoutDigest: Omit<ProviderV1Outcome, "outcomeDigest"> = {
    protocol: providerV1Protocol,
    invocationId,
    status,
    result,
    error,
    effectEvidence,
    hostEffects: emptyProviderHostEffects(),
  };
  return { ...withoutDigest, outcomeDigest: outcomeDigest(withoutDigest) };
};

const assertConformanceRecord = (
  record: ProviderStoredInvocation | undefined,
  invocationId: string,
  requestDigest: string,
  phase: ProviderStoredInvocationPhase,
  outcome: ProviderV1Outcome
): ProviderStoredInvocation => {
  if (!record) {
    throw new Error("Provider invocation Store did not recover its durable record");
  }
  validateStoredProviderInvocation(record, invocationId, requestDigest);
  if (
    record.phase !== phase ||
    canonicalJson(record.outcome) !== canonicalJson(outcome)
  ) {
    throw new Error("Provider invocation Store did not preserve its exact outcome");
  }
  return record;
};

const validateStoredProviderInvocation = (
  record: ProviderStoredInvocation,
  invocationId: string,
  requestDigest?: string
) => {
  const phases: readonly ProviderStoredInvocationPhase[] = [
    "executing",
    "pending",
    "completed",
  ];
  const statuses: readonly ProviderV1OutcomeStatus[] = [
    "pending",
    "succeeded",
    "rejected",
    "failed",
  ];
  if (
    record.invocationId !== invocationId ||
    !validDigest(record.requestDigest) ||
    (requestDigest !== undefined && record.requestDigest !== requestDigest) ||
    record.outcome.protocol !== providerV1Protocol ||
    record.outcome.invocationId !== invocationId ||
    !validDigest(record.outcome.outcomeDigest) ||
    !phases.includes(record.phase) ||
    !statuses.includes(record.outcome.status) ||
    Number.isNaN(Date.parse(record.createdAt)) ||
    Number.isNaN(Date.parse(record.updatedAt)) ||
    Date.parse(record.updatedAt) < Date.parse(record.createdAt) ||
    (record.acknowledgedAt !== null &&
      (Number.isNaN(Date.parse(record.acknowledgedAt)) ||
        Date.parse(record.acknowledgedAt) < Date.parse(record.createdAt)))
  ) {
    throw new Error("Provider invocation Store returned a mismatched record");
  }
  const { outcomeDigest: actualDigest, ...withoutDigest } = record.outcome;
  if (actualDigest !== outcomeDigest(withoutDigest)) {
    throw new Error("Provider invocation Store returned a corrupt outcome");
  }
  if (
    (record.phase === "completed") ===
    (record.outcome.status === "pending")
  ) {
    throw new Error("Provider invocation Store returned an invalid phase");
  }
  if (
    (record.outcome.status === "succeeded" && record.outcome.error !== null) ||
    ((record.outcome.status === "failed" ||
      record.outcome.status === "rejected") &&
      record.outcome.error === null) ||
    (record.outcome.status === "rejected" &&
      record.outcome.error?.retryable) ||
    (record.outcome.status !== "succeeded" &&
      (record.outcome.hostEffects.events.length > 0 ||
        record.outcome.hostEffects.runtimeFunctionRequests.length > 0))
  ) {
    throw new Error("Provider invocation Store returned an invalid outcome");
  }
  if (
    (record.acknowledgedAt === null) !==
      (record.acknowledgedOutcomeDigest === null) ||
    (record.acknowledgedOutcomeDigest !== null &&
      record.acknowledgedOutcomeDigest !== record.outcome.outcomeDigest)
  ) {
    throw new Error(
      "Provider invocation Store returned an invalid acknowledgement"
    );
  }
};

const providerOutcomeStatusCode = (outcome: ProviderV1Outcome) =>
  outcome.status === "pending" ? 202 : 200;

const providerErrorEnvelope = (
  code: string,
  message: string,
  retryable: boolean
) => ({
  error: {
    code,
    message,
    retryable,
    retryAfterMs: null,
    providerTraceReference: null,
    details: [],
  },
});

const validDigest = (value: string) => /^sha256:[0-9a-f]{64}$/u.test(value);

const canonicalDigest = (value: unknown) =>
  `sha256:${createHash("sha256")
    .update(canonicalJson(value))
    .digest("hex")}`;

const validQualifiedId = (value: string) =>
  /^[a-z0-9][a-z0-9._-]*\/[a-z0-9][a-z0-9._-]*$/u.test(value);

const validateProviderV1 = (provider: ProviderV1Options) => {
  if (
    provider.bearerToken !== undefined &&
    (typeof provider.bearerToken !== "string" || !provider.bearerToken.trim())
  ) {
    throw new Error("providerV1.bearerToken must be a non-empty string");
  }
  if (
    provider.features?.includes("durable_invocations") &&
    provider.invocationStore?.durability !== "durable"
  ) {
    throw new Error(
      "providerV1 durable_invocations requires a durable invocationStore"
    );
  }
  if (
    provider.invocationStore &&
    provider.invocationStore.durability !== "durable" &&
    provider.invocationStore.durability !== "process"
  ) {
    throw new Error(
      "providerV1.invocationStore must declare durable or process durability"
    );
  }
  if (provider.invocationStore) {
    for (const method of ["claim", "get", "complete", "acknowledge"] as const) {
      if (typeof provider.invocationStore[method] !== "function") {
        throw new Error(
          `providerV1.invocationStore.${method} must be a function`
        );
      }
    }
  }
  for (const [field, value] of [
    ["protocolContractDigest", provider.protocolContractDigest],
    ["serviceReleaseDigest", provider.serviceReleaseDigest],
  ] as const) {
    if (!validDigest(value)) {
      throw new Error(`providerV1.${field} must be a sha256 digest`);
    }
  }
  for (const field of [
    "serviceId",
    "serviceReleaseVersion",
    "runtimeInstanceId",
  ] as const) {
    if (!provider[field].trim()) {
      throw new Error(`providerV1.${field} must be a non-empty string`);
    }
  }
  if (provider.exports.length === 0) {
    throw new Error("providerV1.exports must contain at least one Module export");
  }
  const keys = new Set<string>();
  if (!validQualifiedId(provider.serviceId)) {
    throw new Error("providerV1.serviceId must use namespace/name");
  }
  for (const providerExport of provider.exports) {
    if (keys.has(providerExport.exportKey)) {
      throw new Error(`providerV1 export ${providerExport.exportKey} is duplicated`);
    }
    keys.add(providerExport.exportKey);
    if (!providerExport.exportKey.trim()) {
      throw new Error("providerV1 exportKey must be non-empty");
    }
    if (!validQualifiedId(providerExport.moduleId)) {
      throw new Error(
        `providerV1 export ${providerExport.exportKey} moduleId must use namespace/name`
      );
    }
    if (canonicalDigest(providerExport.manifest) !== providerExport.manifestDigest) {
      throw new Error(
        `providerV1 export ${providerExport.exportKey} manifestDigest does not match the canonical Manifest`
      );
    }
    for (const [field, value] of [
      ["moduleReleaseDigest", providerExport.moduleReleaseDigest],
      ["manifestDigest", providerExport.manifestDigest],
      ...Object.entries(providerExport.contractDigests),
    ] as const) {
      if (!validDigest(value)) {
        throw new Error(
          `providerV1 export ${providerExport.exportKey} ${field} must be a sha256 digest`
        );
      }
    }
    const moduleRelease = provider.moduleReleases?.[providerExport.exportKey];
    if (
      moduleRelease &&
      canonicalDigest(moduleRelease) !== providerExport.moduleReleaseDigest
    ) {
      throw new Error(
        `providerV1 module release ${providerExport.exportKey} does not match moduleReleaseDigest`
      );
    }
  }
};

const validProviderActor = (
  value: Partial<ProviderActorContext> | null | undefined
) =>
  (value?.kind === "anonymous" &&
    Object.keys(value as object).length === 1) ||
  (value?.kind === "system" && Object.keys(value as object).length === 1) ||
  (value?.kind === "user" &&
    typeof (value as { user_id?: unknown }).user_id === "string" &&
    Array.isArray((value as { scopes?: unknown }).scopes) &&
    (value as { scopes: unknown[] }).scopes.every(
      (scope) => typeof scope === "string"
    ) &&
    Object.keys(value as object).every((key) =>
      ["kind", "user_id", "scopes"].includes(key)
    )) ||
  (value?.kind === "service" &&
    typeof (value as { service_id?: unknown }).service_id === "string" &&
    Array.isArray((value as { scopes?: unknown }).scopes) &&
    (value as { scopes: unknown[] }).scopes.every(
      (scope) => typeof scope === "string"
    ) &&
    Object.keys(value as object).every((key) =>
      ["kind", "service_id", "scopes"].includes(key)
    ));

const providerDescriptor = (
  provider: ProviderV1Options,
  invocationStore: ProviderInvocationStore
) => ({
  exports: provider.exports.map((providerExport) => ({
    ...providerExport,
    readinessReasons: [...(providerExport.readinessReasons ?? [])],
    ready: providerExport.ready ?? true,
  })),
  features: [
    ...new Set([
      ...(provider.features ?? []),
      ...(invocationStore.durability === "durable"
        ? ["durable_invocations"]
        : []),
    ]),
  ],
  protocol: providerV1Protocol,
  protocolContractDigest: provider.protocolContractDigest,
  runtimeInstanceId: provider.runtimeInstanceId,
  serviceId: provider.serviceId,
  serviceReleaseDigest: provider.serviceReleaseDigest,
  serviceReleaseVersion: provider.serviceReleaseVersion,
  transports: ["http_json"],
});

const validateProviderInvocation = (
  value: unknown,
  provider: ProviderV1Options,
  providerExport: ProviderV1Export
): ProviderV1Invocation => {
  const invocation = value as Partial<ProviderV1Invocation> | null;
  const contracts = Object.values(providerExport.contractDigests);
  const operationKinds = [
    "http_route",
    "admin_list",
    "admin_get",
    "admin_query",
    "admin_action",
    "runtime_function",
    "event_handler",
  ];
  if (
    !invocation ||
    invocation.protocol !== providerV1Protocol ||
    !invocation.invocationId?.trim() ||
    !invocation.requestId?.trim() ||
    !Number.isInteger(invocation.attempt) ||
    (invocation.attempt ?? 0) < 1 ||
    typeof invocation.deadline !== "string" ||
    Number.isNaN(Date.parse(invocation.deadline)) ||
    invocation.serviceReleaseDigest !== provider.serviceReleaseDigest ||
    invocation.exportKey !== providerExport.exportKey ||
    invocation.moduleReleaseDigest !== providerExport.moduleReleaseDigest ||
    invocation.manifestDigest !== providerExport.manifestDigest ||
    !invocation.operationKind?.trim() ||
    !operationKinds.includes(invocation.operationKind) ||
    !invocation.operationName?.trim() ||
    !invocation.operationVersion?.trim() ||
    (invocation.mode !== "read_only" && invocation.mode !== "durable") ||
    !validProviderActor(invocation.actor) ||
    !contracts.includes(invocation.inputContractDigest ?? "") ||
    !contracts.includes(invocation.outputContractDigest ?? "") ||
    !invocation.correlationId?.trim() ||
    typeof invocation.trace !== "object" ||
    invocation.trace === null ||
    Array.isArray(invocation.trace) ||
    invocation.contentType !== "application/json"
  ) {
    throw new Error("Provider invocation does not match the locked export");
  }
  if (
    !Object.keys(invocation).every((key) =>
      [
        "protocol",
        "invocationId",
        "requestId",
        "attempt",
        "deadline",
        "serviceReleaseDigest",
        "exportKey",
        "moduleReleaseDigest",
        "manifestDigest",
        "operationKind",
        "operationName",
        "operationVersion",
        "mode",
        "inputContractDigest",
        "outputContractDigest",
        "tenantId",
        "actor",
        "delegation",
        "locale",
        "context",
        "correlationId",
        "causationId",
        "trace",
        "contentType",
        "payload",
      ].includes(key)
    )
  ) {
    throw new Error("Provider invocation contains unknown fields");
  }
  if (
    (invocation.tenantId !== undefined &&
      invocation.tenantId !== null &&
      typeof invocation.tenantId !== "string") ||
    (invocation.locale !== undefined &&
      invocation.locale !== null &&
      typeof invocation.locale !== "string") ||
    (invocation.causationId !== undefined &&
      invocation.causationId !== null &&
      typeof invocation.causationId !== "string") ||
    (invocation.context !== undefined &&
      (typeof invocation.context !== "object" ||
        invocation.context === null ||
        Array.isArray(invocation.context)))
  ) {
    throw new Error("Provider invocation context fields are invalid");
  }
  const validated = invocation as ProviderV1Invocation;
  return {
    protocol: providerV1Protocol,
    invocationId: validated.invocationId,
    requestId: validated.requestId,
    attempt: validated.attempt,
    deadline: validated.deadline,
    serviceReleaseDigest: validated.serviceReleaseDigest,
    exportKey: validated.exportKey,
    moduleReleaseDigest: validated.moduleReleaseDigest,
    manifestDigest: validated.manifestDigest,
    operationKind: validated.operationKind,
    operationName: validated.operationName,
    operationVersion: validated.operationVersion,
    mode: validated.mode,
    inputContractDigest: validated.inputContractDigest,
    outputContractDigest: validated.outputContractDigest,
    tenantId: validated.tenantId ?? null,
    actor: normalizeProviderActor(validated.actor),
    delegation: normalizeJsonValue(validated.delegation ?? null),
    locale: validated.locale ?? null,
    context: normalizeJsonValue(validated.context ?? {}) as Readonly<
      Record<string, unknown>
    >,
    correlationId: validated.correlationId,
    causationId: validated.causationId ?? null,
    trace: normalizeProviderTrace(
      validated.trace as ProviderV1TraceContext
    ),
    contentType: validated.contentType,
    payload: normalizeJsonValue(validated.payload),
  };
};

const invokeProviderV1 = async (
  invocation: ProviderV1Invocation,
  handlers: ProviderAwareServiceModuleHandlers,
  request: IncomingMessage
) => {
  const payload = (invocation.payload ?? {}) as Record<string, unknown>;
  switch (invocation.operationKind) {
    case "http_route": {
      const method = String(payload.method ?? "") as ModuleHttpMethod;
      const declaredPath = String(payload.declared_path ?? "");
      const handler = handlers.http?.[routeKey(method, declaredPath)];
      if (!handler) {
        throw new Error(`${method} ${declaredPath} handler not found`);
      }
      const handlerResult = await handler({
        actor: invocation.actor,
        body: payload.body,
        params: (payload.path_params ?? {}) as Record<string, string>,
        request,
        url: new URL(request.url ?? "", "http://127.0.0.1"),
      });
      if (isProviderHandlerOutcome(handlerResult)) {
        return mapProviderHandlerOutcome(handlerResult, (result) => {
          const normalized = normalizeHandlerResult(result);
          return {
            body: normalized.body,
            status_code: normalized.statusCode,
          };
        });
      }
      const normalized = normalizeHandlerResult(handlerResult);
      return { body: normalized.body, status_code: normalized.statusCode };
    }
    case "admin_list": {
      const data = handlers.data?.[String(payload.entity ?? "")];
      if (!data) throw new Error("admin list handler not found");
      const handlerResult: unknown = await data.list({
        cursor:
          typeof payload.cursor === "string" ? payload.cursor : undefined,
        limit: Number(payload.limit ?? 50),
      });
      return handlerResult;
    }
    case "admin_get": {
      const data = handlers.data?.[String(payload.entity ?? "")];
      if (!data) throw new Error("admin detail handler not found");
      const handlerResult: unknown = await data.detail(String(payload.id ?? ""));
      return isProviderHandlerOutcome(handlerResult)
        ? mapProviderHandlerOutcome(handlerResult, (result) => ({
            record: result,
          }))
        : { record: handlerResult };
    }
    case "admin_query": {
      const query = String(payload.query ?? "");
      const handler = handlers.queries?.[query];
      if (!handler) throw new Error(`${query} query handler not found`);
      const handlerResult = await handler({ query, request });
      return isProviderHandlerOutcome(handlerResult)
        ? mapProviderHandlerOutcome(handlerResult, (result) => ({
            data: result,
          }))
        : { data: handlerResult };
    }
    case "admin_action": {
      const action = String(payload.action ?? "");
      const handler = handlers.actions?.[action];
      if (!handler) throw new Error(`${action} action handler not found`);
      const handlerResult = await handler({
        action,
        input: payload.input,
        request,
      });
      return isProviderHandlerOutcome(handlerResult)
        ? mapProviderHandlerOutcome(handlerResult, (result) => ({
            result,
          }))
        : { result: handlerResult };
    }
    case "runtime_function": {
      const handler = handlers.runtime?.[invocation.operationName];
      if (!handler) throw new Error(`${invocation.operationName} runtime handler not found`);
      const handlerResult = await handler({
        input: payload.input,
        invocation: payload as unknown as ModuleRuntimeInvokeRequest,
        request,
      });
      return isProviderHandlerOutcome(handlerResult)
        ? mapProviderHandlerOutcome(handlerResult, (result) => ({
            output: result,
          }))
        : { output: handlerResult };
    }
    case "event_handler": {
      const handler = handlers.events?.[invocation.operationName];
      if (!handler) throw new Error(`${invocation.operationName} event handler not found`);
      const handlerResult = await handler({
        event: payload as unknown as ModuleEventHandleRequest,
        request,
      });
      return isProviderHandlerOutcome(handlerResult)
        ? mapProviderHandlerOutcome(
            handlerResult,
            (result) => result ?? { actions: [] }
          )
        : handlerResult ?? { actions: [] };
    }
    default:
      throw new Error(`unsupported Provider operation ${invocation.operationKind}`);
  }
};

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
    "content-type": isProblemDetails(body)
      ? "application/problem+json"
      : "application/json; charset=utf-8",
  });
  response.end(JSON.stringify(body));
};

const isProblemDetails = (body: unknown): body is ProblemDetails =>
  typeof body === "object" &&
  body !== null &&
  typeof (body as ProblemDetails).type === "string" &&
  typeof (body as ProblemDetails).title === "string" &&
  typeof (body as ProblemDetails).status === "number" &&
  typeof (body as ProblemDetails).detail === "string" &&
  typeof (body as ProblemDetails).code === "string" &&
  Array.isArray((body as ProblemDetails).errors);

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
      return problemDetails({
        code: "not_found",
        detail: `${declaredRoute.method} ${declaredRoute.path} handler not found`,
        request,
        status: 404,
      });
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
    return problemDetails({
      code: "not_found",
      detail: "runtime function endpoint not found",
      request,
      status: 404,
    });
  }
  const handler = handlers[functionName];
  if (!handler) {
    return problemDetails({
      code: "not_found",
      detail: `${functionName} runtime function handler not found`,
      request,
      status: 404,
    });
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
    return problemDetails({
      code: "not_found",
      detail: "event handler endpoint not found",
      request,
      status: 404,
    });
  }
  const handler = handlers[handlerName];
  if (!handler) {
    return problemDetails({
      code: "not_found",
      detail: `${handlerName} event handler not found`,
      request,
      status: 404,
    });
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
    return problemDetails({
      code: "not_found",
      detail: "admin action endpoint not found",
      request,
      status: 404,
    });
  }
  const handler = handlers[action];
  if (!handler) {
    return problemDetails({
      code: "not_found",
      detail: `${action} admin action handler not found`,
      request,
      status: 404,
    });
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
    return problemDetails({
      code: "not_found",
      detail: "admin query endpoint not found",
      request,
      status: 404,
    });
  }
  const handler = handlers[query];
  if (!handler) {
    return problemDetails({
      code: "not_found",
      detail: `${query} admin query handler not found`,
      request,
      status: 404,
    });
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
  request,
}: {
  basePath: string;
  data: Record<string, ModuleAdminDataSource>;
  request: IncomingMessage;
}): Promise<{ body: unknown; statusCode: number } | null> => {
  const url = new URL(request.url ?? "", "http://127.0.0.1");
  const prefix = `${basePath}/http/admin/`;
  if (!url.pathname.startsWith(prefix)) {
    return null;
  }
  const parts = url.pathname.slice(prefix.length).split("/").filter(Boolean);
  const [entity, id] = parts;
  if (!entity || parts.length > 2) {
    return problemDetails({
      code: "not_found",
      detail: "admin endpoint not found",
      request,
      status: 404,
    });
  }
  const source = data[entity];
  if (!source) {
    return problemDetails({
      code: "not_found",
      detail: `${entity} admin data not found`,
      request,
      status: 404,
    });
  }
  if (id) {
    const record = await source.detail(decodeURIComponent(id));
    return record
      ? { body: { record }, statusCode: 200 }
      : problemDetails({
          code: "not_found",
          detail: `${entity} record ${decodeURIComponent(id)} not found`,
          request,
          status: 404,
        });
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
  const module = defineProviderModule(definition);
  const requires = module.dependencies.map((moduleId) => ({
    capabilities: [],
    module_id: moduleId,
    optional: false,
    version_requirement: "*",
  }));
  const manifest = {
    ...(module.admin === null ? {} : { admin: module.admin }),
    capabilities: module.capabilities,
    console: (module.console ?? []).map((surface) => ({
      ...(surface.icon ? { icon: surface.icon } : {}),
      label: surface.label,
      name: surface.name,
      ...(surface.navigation?.workspace
        ? {
            navigation: {
              ...(surface.navigation.group
                ? { group: surface.navigation.group }
                : {}),
              ...(surface.navigation.order === undefined
                ? {}
                : { order: surface.navigation.order }),
              workspace: surface.navigation.workspace,
            },
          }
        : {}),
      presentation: {
        entry: surface.package.export,
        kind: "esm" as const,
      },
      required_capabilities: surface.required_capabilities ?? [],
      route: surface.route,
    })),
    console_contributions: [],
    console_slots: [],
    ...(module.events ? { events: module.events } : {}),
    http_routes: module.http_routes,
    ...(module.lifecycle ? { lifecycle: module.lifecycle } : {}),
    module_id: module.name,
    protocol: "lenso.module-manifest.v1",
    ...(requires.length === 0 ? {} : { requires }),
    runtime: {
      functions: module.runtime.functions,
      schedules: [],
    },
    story_display: module.story_display,
  };
  Object.defineProperties(manifest, {
    ...(module.admin === null ? { admin: { value: module.admin } } : {}),
    dependencies: { value: module.dependencies },
    name: { value: module.name },
    ...(requires.length === 0 ? { requires: { value: requires } } : {}),
    version: { value: module.version },
  });
  return manifest as unknown as ServiceModuleManifest;
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
    if (!module.module_id.trim()) {
      throw new Error("Service module name is required");
    }
    if (moduleNames.has(module.module_id)) {
      throw new Error(`Duplicate service module: ${module.module_id}`);
    }
    moduleNames.add(module.module_id);
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
    name: module.module_id,
    version: manifest.version,
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
  admin: module.admin,
  capabilities: module.capabilities,
  ...(service.compatibility ? { compatibility: service.compatibility } : {}),
  console: [],
  dependencies: module.requires.map((requirement) => requirement.module_id),
  ...(module.events ? { events: module.events } : {}),
  http_routes: module.http_routes,
  ...(module.lifecycle ? { lifecycle: module.lifecycle } : {}),
  name: module.module_id,
  runtime: module.runtime,
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
  story_display: module.story_display,
  version: service.version,
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
        request,
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

    const notFound = problemDetails({
      code: "not_found",
      detail: `${manifest.name} service module endpoint not found`,
      request,
      status: 404,
    });
    sendJson(response, notFound.statusCode, notFound.body);
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

type ServeServiceFunction = {
  (
    manifest: ServiceManifest,
    options?: ProviderAwareServeServiceOptions
  ): Promise<ServedService>;
  (
    manifest: ServiceManifest,
    options?: ServeServiceOptions
  ): Promise<ServedService>;
};

// ponytail: shared server wrapper is intentionally flat; split handlers if this grows again.
// eslint-disable-next-line complexity
export const serveService: ServeServiceFunction = async (
  manifest: ServiceManifest,
  options: ServeServiceOptions | ProviderAwareServeServiceOptions = {}
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
  const providerV1 = options.providerV1;
  if (providerV1) validateProviderV1(providerV1);
  const providerBearer = providerV1?.bearerToken
    ? Buffer.from(providerV1.bearerToken, "utf8")
    : undefined;
  if (
    providerV1 &&
    host !== "127.0.0.1" &&
    host !== "localhost" &&
    !providerBearer
  ) {
    throw new Error(
      "Provider V1 requires providerV1.bearerToken outside loopback"
    );
  }
  const providerInvocationStore = providerV1
    ? providerV1.invocationStore ?? createMemoryProviderInvocationStore()
    : undefined;
  let servedBaseUrl = "";

  const server = createServer(async (request, response) => {
    const requestPath = new URL(request.url ?? "", "http://127.0.0.1").pathname;
    if (
      providerV1 &&
      providerBearer &&
      (requestPath === providerV1BasePath ||
        requestPath.startsWith(`${providerV1BasePath}/`)) &&
      !localBearerMatches(request, providerBearer)
    ) {
      sendJson(
        response,
        401,
        providerErrorEnvelope(
          "provider_bearer_required",
          "Provider V1 access requires the configured bearer credential",
          false
        )
      );
      return;
    }
    if (providerV1 && request.method === "GET" && requestPath === providerV1BasePath) {
      sendJson(
        response,
        200,
        providerDescriptor(providerV1, providerInvocationStore!)
      );
      return;
    }
    if (
      providerV1 &&
      request.method === "GET" &&
      requestPath.startsWith(`${providerV1BasePath}/exports/`) &&
      requestPath.endsWith("/module-release")
    ) {
      const encodedExportKey = requestPath.slice(
        `${providerV1BasePath}/exports/`.length,
        -"/module-release".length
      );
      const exportKey = decodeURIComponent(encodedExportKey);
      const moduleRelease = providerV1.moduleReleases?.[exportKey];
      if (moduleRelease) {
        sendJson(response, 200, moduleRelease);
      } else {
        sendJson(response, 404, {
          error: {
            code: "module_release_not_found",
            details: [],
            message: `Provider export ${exportKey} does not expose an exact Module Release`,
            retryable: false,
          },
        });
      }
      return;
    }
    if (
      providerV1 &&
      request.method === "GET" &&
      (requestPath === `${providerV1BasePath}/health/live` ||
        requestPath === `${providerV1BasePath}/health/ready`)
    ) {
      sendJson(response, 200, {
        exports: Object.fromEntries(
          providerV1.exports.map((entry) => [entry.exportKey, { ready: entry.ready ?? true, reasons: [...(entry.readinessReasons ?? [])] }])
        ),
        live: true,
        observedAt: new Date().toISOString(),
        protocol: providerV1Protocol,
        ready: providerV1.exports.every((entry) => entry.ready ?? true),
        serviceId: providerV1.serviceId,
        serviceReleaseDigest: providerV1.serviceReleaseDigest,
      });
      return;
    }
    if (providerV1 && requestPath.startsWith(`${providerV1BasePath}/invocations/`)) {
      const suffix = requestPath.slice(`${providerV1BasePath}/invocations/`.length);
      const acknowledgement = suffix.endsWith(":ack");
      const invocationId = decodeURIComponent(acknowledgement ? suffix.slice(0, -4) : suffix);
      if (request.method === "GET" && !acknowledgement) {
        try {
          const record = await providerInvocationStore!.get(invocationId);
          if (!record) {
            sendJson(
              response,
              404,
              providerErrorEnvelope(
                "not_found",
                "Invocation not found",
                false
              )
            );
            return;
          }
          validateStoredProviderInvocation(record, invocationId);
          sendJson(
            response,
            providerOutcomeStatusCode(record.outcome),
            record.outcome
          );
        } catch {
          sendJson(
            response,
            503,
            providerErrorEnvelope(
              "invocation_store_unavailable",
              "Provider invocation Store is unavailable",
              true
            )
          );
        }
        return;
      }
      if (request.method === "POST" && acknowledgement) {
        const value = await readBody(request);
        const body =
          typeof value === "object" && value !== null
            ? (value as {
                invocationId?: string;
                outcomeDigest?: string;
              })
            : {};
        if (
          body.invocationId !== invocationId ||
          typeof body.outcomeDigest !== "string"
        ) {
          sendJson(
            response,
            409,
            providerErrorEnvelope(
              "acknowledgement_conflict",
              "Acknowledgement does not match the durable outcome",
              false
            )
          );
          return;
        }
        try {
          const acknowledged = await providerInvocationStore!.acknowledge({
            invocationId,
            now: new Date().toISOString(),
            outcomeDigest: body.outcomeDigest,
          });
          if (acknowledged.kind !== "acknowledged") {
            sendJson(
              response,
              409,
              providerErrorEnvelope(
                "acknowledgement_conflict",
                "Acknowledgement does not match the durable outcome",
                false
              )
            );
            return;
          }
          validateStoredProviderInvocation(
            acknowledged.record,
            invocationId
          );
          sendJson(response, 200, { invocationId });
        } catch {
          sendJson(
            response,
            503,
            providerErrorEnvelope(
              "invocation_store_unavailable",
              "Provider invocation Store is unavailable",
              true
            )
          );
        }
        return;
      }
    }
    if (providerV1 && request.method === "POST" && requestPath.startsWith(`${providerV1BasePath}/exports/`)) {
      let invocation: ProviderV1Invocation;
      let providerExport: ProviderV1Export;
      try {
        const [encodedExportKey] = requestPath.slice(`${providerV1BasePath}/exports/`.length).split("/");
        const exportKey = decodeURIComponent(encodedExportKey ?? "");
        const foundExport = providerV1.exports.find((entry) => entry.exportKey === exportKey);
        if (!foundExport) throw new Error(`Provider export ${exportKey} is not declared`);
        providerExport = foundExport;
        invocation = validateProviderInvocation(
          await readBody(request),
          providerV1,
          providerExport
        );
      } catch (error) {
        sendJson(
          response,
          400,
          providerErrorEnvelope(
            "invalid_invocation",
            error instanceof Error
              ? error.message
              : "Provider invocation is invalid",
            false
          )
        );
        return;
      }
      const requestDigest = canonicalDigest(invocation);
      const pendingOutcome = providerOutcome(
        invocation,
        providerPending(),
        providerExport
      );
      let claim: ProviderInvocationStoreClaimResult;
      try {
        claim = await providerInvocationStore!.claim({
          invocationId: invocation.invocationId,
          now: new Date().toISOString(),
          pendingOutcome,
          requestDigest,
        });
      } catch {
        sendJson(
          response,
          503,
          providerErrorEnvelope(
            "invocation_store_unavailable",
            "Provider invocation Store is unavailable",
            true
          )
        );
        return;
      }
      if (claim.kind === "conflict") {
        sendJson(
          response,
          409,
          providerErrorEnvelope(
            "invocation_identity_conflict",
            "Invocation id is already bound to a different canonical request",
            false
          )
        );
        return;
      }
      try {
        validateStoredProviderInvocation(
          claim.record,
          invocation.invocationId,
          requestDigest
        );
      } catch {
        sendJson(
          response,
          503,
          providerErrorEnvelope(
            "invocation_store_unavailable",
            "Provider invocation Store is unavailable",
            true
          )
        );
        return;
      }
      if (claim.kind === "replay") {
        sendJson(
          response,
          providerOutcomeStatusCode(claim.record.outcome),
          claim.record.outcome
        );
        return;
      }
      const providerModules = options.modules as
        | Record<string, ProviderAwareServiceModuleHandlers>
        | undefined;
      const handlers =
        providerModules?.[providerExport.moduleId] ??
        providerModules?.[providerExport.exportKey] ??
        {};
      let outcome: ProviderV1Outcome;
      try {
        const result = await invokeProviderV1(invocation, handlers, request);
        outcome = providerOutcome(invocation, result, providerExport);
      } catch {
        outcome = providerOutcome(
          invocation,
          providerFailed({
            code: "provider_handler_failed",
            message: "Provider handler failed",
            retryable: false,
          }),
          providerExport
        );
      }
      try {
        const completed = await providerInvocationStore!.complete({
          invocationId: invocation.invocationId,
          now: new Date().toISOString(),
          outcome,
          requestDigest,
        });
        validateStoredProviderInvocation(
          completed,
          invocation.invocationId,
          requestDigest
        );
        sendJson(
          response,
          providerOutcomeStatusCode(completed.outcome),
          completed.outcome
        );
      } catch {
        sendJson(
          response,
          503,
          providerErrorEnvelope(
            "invocation_store_unavailable",
            "Provider invocation Store is unavailable",
            true
          )
        );
      }
      return;
    }
    if (
      request.method === "GET" &&
      requestPath === systemPlaneCorePath &&
      coreDocument &&
      coreBearer
    ) {
      if (!localBearerMatches(request, coreBearer)) {
        const unauthorized = problemDetails({
          code: "system_plane_bearer_required",
          detail:
            "System Plane Core access requires the configured local bearer credential",
          request,
          status: 401,
        });
        sendJson(response, unauthorized.statusCode, unauthorized.body);
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
      const moduleBasePath = `${basePath}/modules/${module.module_id}`;
      const moduleHandlers = (options.modules?.[module.module_id] ??
        {}) as ServiceModuleHandlers;
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
          request,
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

    const notFound = problemDetails({
      code: "not_found",
      detail: `${manifest.name} service endpoint not found`,
      request,
      status: 404,
    });
    sendJson(response, notFound.statusCode, notFound.body);
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
    const notFound = problemDetails({
      code: "not_found",
      detail: `${method} ${declaredPath} handler not found`,
      status: 404,
    });
    return {
      body: notFound.body,
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
