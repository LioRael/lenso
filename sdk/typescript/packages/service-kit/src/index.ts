/* eslint-disable func-style, no-use-before-define, sort-keys */
export {
  actionBooleanField,
  actionConfirmation,
  actionIntegerField,
  actionJsonField,
  actionTextField,
  actionTimestampField,
  adminAction,
  adminSchema,
  booleanField,
  declarativeCustom,
  declarativePage,
  declarativeSection,
  defineModule,
  defineSchemaEntity,
  defineService,
  deleteRoute,
  embeddedCustom,
  entityDetail,
  entityTable,
  eventHandler,
  everyStartup,
  getRoute,
  integerField,
  jsonField,
  lifecycle,
  metricBinding,
  metricStrip,
  patchRoute,
  postRoute,
  putRoute,
  queryValue,
  readLensoInvocationContext,
  runtimeFunction,
  schemaAdmin,
  serveService,
  textField,
  timestampField,
} from "./service-module-delivery.js";
export type {
  ActionFieldOptions,
  AdminAction,
  AdminActionConfirmation,
  AdminActionDangerLevel,
  AdminActionInputField,
  AdminActionInputSchema,
  AdminActionOptions,
  AdminConfirmationOptions,
  AdminDeclarativeComponent,
  AdminDeclarativePage,
  AdminDeclarativePageOptions,
  AdminDeclarativeSection,
  AdminDeclarativeSectionOptions,
  AdminDeclarativeSurface,
  AdminDeclarativeSurfaceOptions,
  AdminEmbeddedRuntime,
  AdminEmbeddedSurface,
  AdminMetricBinding,
  AdminSchema,
  LensoInvocationContext,
  ModuleEventHandleRequest,
  ModuleEventHandleResponse,
  ModuleEventHandler,
  ModuleEventHandlerContext,
  ModuleEventHandlerDeclaration,
  ModuleEventHandlerOptions,
  ModuleHttpHandler,
  ModuleHttpHandlerContext,
  ModuleHttpHandlerResult,
  ModuleHttpMethod,
  ModuleHttpRoute,
  ModuleHttpRouteOptions,
  ModuleLifecycleActivationJob,
  ModuleLifecycleActivationOptions,
  ModuleLifecycleStartupCheck,
  ModuleLifecycleSurface,
  ModuleRuntimeFunctionDeclaration,
  ModuleRuntimeFunctionOptions,
  ModuleRuntimeHandler,
  ModuleRuntimeHandlerContext,
  ModuleRuntimeInvokeRequest,
  ModuleRuntimeRetryPolicy,
  ModuleStoryDisplayDescriptor,
  ModuleStoryDisplaySource,
  SchemaAdminSurface,
  SchemaEntity,
  SchemaField,
  SchemaFieldType,
  ServedService,
  ServiceDefinition,
  ServiceInstall,
  ServiceInstallCommand,
  ServiceInstallService,
  ServiceManifest,
  ServiceModuleDefinition,
  ServiceModuleHandlers,
  ServiceModuleManifest,
  ServiceOperationIdempotency,
  ServiceOperationMetadata,
  ServiceOperationSafeProbe,
  ServiceStatus,
  ServiceStatusCheck,
  ServiceStatusOptions,
  ServiceStatusState,
  ServeServiceOptions,
} from "./service-module-delivery.js";

export interface ServiceContract {
  name: string;
  version?: string;
  provider?: ServiceProviderMetadata;
  compatibility?: ServiceCompatibility;
  config?: ServiceConfigField[];
  env?: ServiceEnvField[];
  health?: ServiceHealth;
  localProcess?: ServiceLocalProcess;
  deployment?: ServiceDeployment;
  modules: ServiceModuleContract[];
}

export interface ServiceProviderMetadata {
  name: string;
  vendor?: string;
  summary?: string;
  homepage?: string;
}

export interface ServiceCompatibility {
  providerProtocolVersion?: string;
  requiredHostFeatures?: string[];
  sdkLanguage?: "ts" | "rust" | string;
  sdkVersion?: string;
}

export interface ServiceConfigField {
  key: string;
  required?: boolean;
  defaultValue?: unknown;
  secret?: boolean;
}

export interface ServiceEnvField {
  name: string;
  required?: boolean;
  example?: string;
}

export interface ServiceHealth {
  manifestUrl?: string;
  readyUrl?: string;
  livenessUrl?: string;
  statusUrl?: string;
}

export interface ServiceLocalProcess {
  command: string;
  cwd?: string;
  env?: Record<string, string>;
  autoStart?: boolean;
  readyTimeoutMs?: number;
}

export interface ServiceDeployment {
  target: "kubernetes" | "container-paas" | "compose" | "systemd" | string;
  commands?: string[];
  composeService?: string;
  kubernetes?: KubernetesDeployment;
}

export interface KubernetesDeployment {
  port?: number;
  replicas?: number;
  ingressHost?: string;
  env?: string[];
  secrets?: string[];
  autoscaling?: boolean;
  disruptionBudget?: boolean;
  networkPolicy?: boolean;
}

export interface ServiceModuleContract {
  name: string;
  version?: string;
  capabilities?: string[];
  dependencies?: string[];
}

export interface ServiceContractIssue {
  path: string;
  message: string;
}

export const servicePackageProtocol = "lenso.service-package.v1" as const;
export const serviceWorkspaceProtocol = "lenso.service-workspace.v1" as const;
export const serviceReleasePlanProtocol =
  "lenso.service-release-plan.v1" as const;
export const moduleContractProtocol = "lenso.module.v1" as const;
export const moduleReleaseProtocol = "lenso.module-release.v1" as const;
export type ModuleArtifactSource = "linked" | "service" | "bundled";
export type ServiceReleaseRisk =
  | "safe"
  | "needs_attention"
  | "breaking"
  | "blocked";

export interface ServicePackage {
  protocol: typeof servicePackageProtocol;
  name: string;
  version: string;
  serviceManifest: string;
  modules: string[];
}

export interface ServiceWorkspace {
  protocol: typeof serviceWorkspaceProtocol;
  services: ServiceWorkspaceService[];
}

export interface ServiceWorkspaceService {
  name: string;
  lang: "ts" | "rust" | string;
  cwd: string;
  manifest: string;
  command: string;
  readyUrl: string;
  autoStart?: boolean;
  readyTimeoutMs?: number;
  modules?: string[];
}

export interface ServiceWorkspaceProcess {
  name: string;
  command: string;
  cwd: string;
  readyUrl: string;
  autoStart: boolean;
  readyTimeoutMs: number;
}

export interface ServiceWorkspaceModuleServices {
  moduleName: string;
  services: ServiceWorkspaceProcess[];
}

export interface ServiceWorkspaceModuleServicesFile {
  version: 1;
  modules: ServiceWorkspaceModuleServices[];
}

export interface ServiceReleaseChangeSet {
  added: string[];
  removed: string[];
}

export interface ServiceReleaseModuleChangeSet extends ServiceReleaseChangeSet {
  module: string;
}

export interface ServiceReleaseDiff {
  capabilities: ServiceReleaseModuleChangeSet[];
  compatibilityChanged?: boolean;
  config: ServiceReleaseChangeSet;
  env: ServiceReleaseChangeSet;
  modules: ServiceReleaseChangeSet;
  operations: ServiceReleaseModuleChangeSet[];
}

export interface ServiceReleaseManifestSummary {
  name: string;
  version?: string;
  manifestReference: string;
  packageReference?: string;
  inputReference?: string;
  modules: string[];
  compatibilityIssue?: string | null;
}

export interface ServiceReleasePolicyIssue {
  code: string;
  level: Exclude<ServiceReleaseRisk, "safe">;
  message: string;
}

export interface ServiceReleasePolicy {
  risk: ServiceReleaseRisk;
  issues: ServiceReleasePolicyIssue[];
}

export interface ServiceReleasePlan {
  protocol: typeof serviceReleasePlanProtocol;
  service: { name: string };
  current: ServiceReleaseManifestSummary;
  candidate: ServiceReleaseManifestSummary;
  diff: ServiceReleaseDiff;
  policy: ServiceReleasePolicy;
  restartRequired: boolean;
  nextAction: string;
  createdAtUnixMs?: number;
}

export interface ModuleReleaseProvider {
  name: string;
  servicePackage?: string;
  serviceManifest?: string;
}

export interface ModuleContract {
  protocol: typeof moduleContractProtocol;
  name: string;
  version: string;
  source: ModuleArtifactSource;
  summary?: string;
  capabilities?: string[];
  dependencies?: string[];
  manifest?: ServiceModuleContract & Record<string, unknown>;
}

export interface ModuleRelease {
  protocol: typeof moduleReleaseProtocol;
  name: string;
  version: string;
  source: ModuleArtifactSource;
  provider?: ModuleReleaseProvider;
  summary?: string;
  capabilities?: string[];
  dependencies?: string[];
  linked?: Record<string, unknown>;
}

export const serviceContractSchema = {
  $id: "https://contracts.lenso.local/services/lenso-service.v1.schema.json",
  $schema: "https://json-schema.org/draft/2020-12/schema",
  additionalProperties: true,
  properties: {
    compatibility: { type: "object" },
    config: { type: "array" },
    env: { type: "array" },
    deployment: { type: "object" },
    health: { type: "object" },
    localProcess: { type: "object" },
    modules: { minItems: 1, type: "array" },
    name: { minLength: 1, type: "string" },
    provider: { type: "object" },
    version: { minLength: 1, type: "string" },
  },
  required: ["name", "modules"],
  title: "LensoServiceContract",
  type: "object",
} as const;

export const servicePackageSchema = {
  $id: "https://contracts.lenso.local/services/lenso-service-package.v1.schema.json",
  $schema: "https://json-schema.org/draft/2020-12/schema",
  additionalProperties: true,
  properties: {
    modules: {
      items: { minLength: 1, type: "string" },
      minItems: 1,
      type: "array",
    },
    name: { minLength: 1, type: "string" },
    protocol: { const: servicePackageProtocol, type: "string" },
    serviceManifest: { minLength: 1, type: "string" },
    version: { minLength: 1, type: "string" },
  },
  required: ["protocol", "name", "version", "serviceManifest", "modules"],
  title: "LensoServicePackage",
  type: "object",
} as const;

export const serviceWorkspaceSchema = {
  $id: "https://contracts.lenso.local/services/lenso-service-workspace.v1.schema.json",
  $schema: "https://json-schema.org/draft/2020-12/schema",
  additionalProperties: true,
  properties: {
    protocol: { const: serviceWorkspaceProtocol, type: "string" },
    services: {
      items: {
        additionalProperties: true,
        properties: {
          autoStart: { type: "boolean" },
          command: { minLength: 1, type: "string" },
          cwd: { minLength: 1, type: "string" },
          lang: { minLength: 1, type: "string" },
          manifest: { minLength: 1, type: "string" },
          modules: { items: { minLength: 1, type: "string" }, type: "array" },
          name: { minLength: 1, type: "string" },
          readyTimeoutMs: { minimum: 1, type: "integer" },
          readyUrl: { minLength: 1, type: "string" },
        },
        required: ["name", "lang", "cwd", "manifest", "command", "readyUrl"],
        type: "object",
      },
      type: "array",
    },
  },
  required: ["protocol"],
  title: "LensoServiceWorkspace",
  type: "object",
} as const;

export const moduleReleaseSchema = {
  $id: "https://contracts.lenso.local/modules/lenso-module-release.v1.schema.json",
  $schema: "https://json-schema.org/draft/2020-12/schema",
  additionalProperties: true,
  allOf: [
    {
      if: {
        properties: { source: { const: "service" } },
        required: ["source"],
      },
      // eslint-disable-next-line unicorn/no-thenable
      then: { required: ["provider"] },
    },
  ],
  properties: {
    capabilities: { items: { minLength: 1, type: "string" }, type: "array" },
    dependencies: { items: { minLength: 1, type: "string" }, type: "array" },
    name: { minLength: 1, type: "string" },
    protocol: { const: moduleReleaseProtocol, type: "string" },
    provider: {
      additionalProperties: true,
      anyOf: [
        { required: ["servicePackage"] },
        { required: ["serviceManifest"] },
      ],
      properties: {
        name: { minLength: 1, type: "string" },
        serviceManifest: { minLength: 1, type: "string" },
        servicePackage: { minLength: 1, type: "string" },
      },
      required: ["name"],
      type: "object",
    },
    source: { enum: ["linked", "service", "bundled"], type: "string" },
    summary: { type: "string" },
    version: { minLength: 1, type: "string" },
  },
  required: ["protocol", "name", "version", "source"],
  title: "LensoModuleRelease",
  type: "object",
} as const;

export const moduleContractSchema = {
  $id: "https://contracts.lenso.local/modules/lenso-module.v1.schema.json",
  $schema: "https://json-schema.org/draft/2020-12/schema",
  additionalProperties: true,
  properties: {
    capabilities: { items: { minLength: 1, type: "string" }, type: "array" },
    dependencies: { items: { minLength: 1, type: "string" }, type: "array" },
    manifest: { type: "object" },
    name: { minLength: 1, type: "string" },
    protocol: { const: moduleContractProtocol, type: "string" },
    source: { enum: ["linked", "service", "bundled"], type: "string" },
    summary: { type: "string" },
    version: { minLength: 1, type: "string" },
  },
  required: ["protocol", "name", "version", "source"],
  title: "LensoModuleContract",
  type: "object",
} as const;

export function defineServiceContract(
  contract: ServiceContract
): ServiceContract {
  return {
    ...contract,
    config: contract.config ?? [],
    env: contract.env ?? [],
    modules: contract.modules,
  };
}

export function defineServicePackage(
  servicePackage: Omit<ServicePackage, "protocol" | "serviceManifest"> & {
    serviceManifest?: string;
  }
): ServicePackage {
  return {
    ...servicePackage,
    protocol: servicePackageProtocol,
    serviceManifest: servicePackage.serviceManifest ?? "lenso.service.json",
  };
}

export function defineServiceWorkspace(
  workspace: Omit<ServiceWorkspace, "protocol" | "services"> & {
    protocol?: string;
    services: (Omit<ServiceWorkspaceService, "manifest"> & {
      manifest?: string;
    })[];
  }
): ServiceWorkspace {
  return {
    ...workspace,
    protocol: serviceWorkspaceProtocol,
    services: workspace.services.map((service) => ({
      ...service,
      autoStart: service.autoStart ?? true,
      manifest: service.manifest || "lenso.service.json",
      readyTimeoutMs: service.readyTimeoutMs ?? 10_000,
    })),
  };
}

export function serviceWorkspaceToModuleServices(
  workspace: ServiceWorkspace
): ServiceWorkspaceModuleServicesFile {
  return {
    modules: workspace.services.map((service) => ({
      moduleName: service.name,
      services: [
        {
          autoStart: service.autoStart ?? true,
          command: service.command,
          cwd: service.cwd,
          name: service.name,
          readyTimeoutMs: service.readyTimeoutMs ?? 10_000,
          readyUrl: service.readyUrl,
        },
      ],
    })),
    version: 1,
  };
}

export function defineServiceReleasePlan(
  plan: Omit<ServiceReleasePlan, "protocol" | "policy" | "nextAction"> & {
    policy?: ServiceReleasePolicy;
    nextAction?: string;
  }
): ServiceReleasePlan {
  const policy =
    plan.policy ??
    evaluateServiceReleasePolicy(
      plan.diff,
      plan.candidate.compatibilityIssue ?? undefined
    );
  return {
    ...plan,
    nextAction: plan.nextAction ?? serviceReleaseNextAction(policy.risk),
    policy,
    protocol: serviceReleasePlanProtocol,
  };
}

export function evaluateServiceReleasePolicy(
  diff: ServiceReleaseDiff,
  compatibilityIssue?: string | null
): ServiceReleasePolicy {
  const issues: ServiceReleasePolicyIssue[] = [];
  if (compatibilityIssue) {
    issues.push({
      code: "host_incompatible",
      level: "blocked",
      message: compatibilityIssue,
    });
  } else if (diff.compatibilityChanged) {
    issues.push({
      code: "compatibility_changed",
      level: "needs_attention",
      message:
        "Service compatibility metadata changed; review host support before applying.",
    });
  }
  for (const moduleName of diff.modules.removed) {
    issues.push({
      code: "module_removed",
      level: "breaking",
      message: `Module \`${moduleName}\` is removed by this release.`,
    });
  }
  for (const envName of diff.env.added) {
    issues.push({
      code: "env_added",
      level: "needs_attention",
      message: `Environment value \`${envName}\` is newly required by this release.`,
    });
  }
  for (const configKey of diff.config.added) {
    issues.push({
      code: "config_added",
      level: "needs_attention",
      message: `Runtime config \`${configKey}\` is newly declared by this release.`,
    });
  }
  for (const change of diff.capabilities) {
    for (const capability of change.removed) {
      issues.push({
        code: "capability_removed",
        level: "breaking",
        message: `Capability \`${capability}\` is removed from module \`${change.module}\`.`,
      });
    }
  }
  for (const change of diff.operations) {
    for (const operation of change.removed) {
      issues.push({
        code: "operation_removed",
        level: "breaking",
        message: `Operation \`${operation}\` is removed from module \`${change.module}\`.`,
      });
    }
  }
  return {
    issues,
    risk:
      issues
        .map((issue) => issue.level)
        .toSorted(
          (left, right) => releaseRiskRank(right) - releaseRiskRank(left)
        )
        .at(0) ?? "safe",
  };
}

export function serviceReleaseRestartRequired(
  diff: ServiceReleaseDiff
): boolean {
  return (
    diff.compatibilityChanged === true ||
    diff.modules.added.length > 0 ||
    diff.modules.removed.length > 0 ||
    diff.env.added.length > 0 ||
    diff.env.removed.length > 0 ||
    diff.config.added.length > 0 ||
    diff.config.removed.length > 0 ||
    diff.capabilities.some(
      (change) => change.added.length > 0 || change.removed.length > 0
    ) ||
    diff.operations.some(
      (change) => change.added.length > 0 || change.removed.length > 0
    )
  );
}

export function serviceWorkspaceBaseUrl(
  service: Pick<ServiceWorkspaceService, "manifest" | "readyUrl">
): string | undefined {
  return (
    serviceBaseUrlFromReadyUrl(service.readyUrl) ??
    serviceBaseUrlFromManifestUrl(service.manifest)
  );
}

export function serviceBaseUrlFromReadyUrl(
  readyUrl: string
): string | undefined {
  const url = parseUrl(readyUrl);
  if (!url) {
    return undefined;
  }
  const path = url.pathname.replace(/\/+$/u, "");
  const basePath = ["/status", "/ready", "/health", "/healthz"]
    .map((suffix) =>
      path.endsWith(suffix) ? path.slice(0, -suffix.length) : undefined
    )
    .find((value): value is string => value !== undefined);
  if (basePath === undefined) {
    return undefined;
  }
  url.pathname = basePath;
  url.search = "";
  url.hash = "";
  return trimTrailingSlash(url.toString());
}

export function serviceBaseUrlFromManifestUrl(
  manifestUrl: string
): string | undefined {
  const url = parseUrl(manifestUrl);
  if (!url) {
    return undefined;
  }
  const path = url.pathname.replace(/\/+$/u, "");
  if (!path.endsWith("/manifest")) {
    return undefined;
  }
  url.pathname = path.slice(0, -"/manifest".length);
  url.search = "";
  url.hash = "";
  return trimTrailingSlash(url.toString());
}

export function defineModuleContract(
  contract: Omit<ModuleContract, "protocol">
): ModuleContract {
  return {
    ...contract,
    protocol: moduleContractProtocol,
  };
}

export function defineModuleRelease(
  release: Omit<ModuleRelease, "protocol" | "source"> & {
    source?: ModuleArtifactSource;
  }
): ModuleRelease {
  const source = release.source ?? "service";
  const provider = moduleReleaseProviderForSource(source, release.provider);
  const { provider: _provider, source: _source, ...rest } = release;
  if (provider) {
    return {
      ...rest,
      protocol: moduleReleaseProtocol,
      provider,
      source,
    };
  }
  return {
    ...rest,
    protocol: moduleReleaseProtocol,
    source,
  };
}

function moduleReleaseProviderForSource(
  source: ModuleArtifactSource,
  provider: ModuleReleaseProvider | undefined
): ModuleReleaseProvider | undefined {
  if (source !== "service" || !provider) {
    return provider;
  }
  if (provider.servicePackage || provider.serviceManifest) {
    return provider;
  }
  return { ...provider, servicePackage: "lenso.service-package.json" };
}

function releaseRiskRank(risk: ServiceReleaseRisk): number {
  return {
    blocked: 3,
    breaking: 2,
    needs_attention: 1,
    safe: 0,
  }[risk];
}

function serviceReleaseNextAction(risk: ServiceReleaseRisk): string {
  if (risk === "safe") {
    return "Run `lenso service release apply <plan.json>` when ready.";
  }
  if (risk === "needs_attention") {
    return "Review required env/config, then run `lenso service release apply <plan.json>`.";
  }
  if (risk === "breaking") {
    return "Review removed modules, capabilities, or operations before applying.";
  }
  return "Fix blocked policy issues before applying this release.";
}

function parseUrl(value: string): URL | undefined {
  try {
    return new URL(value);
  } catch {
    return undefined;
  }
}

function trimTrailingSlash(value: string): string {
  return value.replace(/\/+$/u, "");
}

export function serviceEnv(
  name: string,
  options: Omit<ServiceEnvField, "name"> = {}
): ServiceEnvField {
  return { name, ...options };
}

export function serviceHealth(health: ServiceHealth): ServiceHealth {
  return health;
}

export function defineKubernetesDeployment(
  kubernetes: KubernetesDeployment
): ServiceDeployment {
  return { kubernetes, target: "kubernetes" };
}

export function validateServiceContract(
  value: unknown
): ServiceContractIssue[] {
  const root = asRecord(value);
  if (!root) {
    return [{ message: "service contract must be an object", path: "$" }];
  }

  const issues: ServiceContractIssue[] = [];
  requireNonEmptyString(root.name, "$.name", issues);
  if ("version" in root) {
    requireNonEmptyString(root.version, "$.version", issues);
  }
  validateProvider(root.provider, issues);
  validateNamedFieldsArray(root.config, "$.config", "key", issues);
  validateNamedFieldsArray(root.env, "$.env", "name", issues);
  validateStringArray(
    root.requiredEnv ?? root.required_env,
    "$.requiredEnv",
    issues
  );
  validateCompatibility(root.compatibility, issues);
  validateLocalProcess(
    root.localProcess ?? root.local_process,
    "$.localProcess",
    issues
  );
  validateInstall(root.install, issues);
  validateModules(root.modules, issues);
  return issues;
}

export function assertServiceContract(
  value: unknown
): asserts value is ServiceContract {
  const issues = validateServiceContract(value);
  if (issues.length > 0) {
    throw new Error(
      `Invalid Lenso service contract: ${issues
        .map((issue) => `${issue.path} ${issue.message}`)
        .join("; ")}`
    );
  }
}

export function validateServicePackage(value: unknown): ServiceContractIssue[] {
  const root = asRecord(value);
  if (!root) {
    return [{ message: "service package must be an object", path: "$" }];
  }

  const issues: ServiceContractIssue[] = [];
  if (root.protocol !== servicePackageProtocol) {
    issues.push({
      message: `protocol must be \`${servicePackageProtocol}\``,
      path: "$.protocol",
    });
  }
  requireNonEmptyString(root.name, "$.name", issues);
  requireNonEmptyString(root.version, "$.version", issues);
  requireNonEmptyString(
    root.serviceManifest ?? root.service_manifest,
    "$.serviceManifest",
    issues
  );
  validateServicePackageModules(root.modules, issues);
  return issues;
}

export function assertServicePackage(
  value: unknown
): asserts value is ServicePackage {
  const issues = validateServicePackage(value);
  if (issues.length > 0) {
    throw new Error(
      `Invalid Lenso service package: ${issues
        .map((issue) => `${issue.path} ${issue.message}`)
        .join("; ")}`
    );
  }
}

export function validateServiceWorkspace(
  value: unknown
): ServiceContractIssue[] {
  const root = asRecord(value);
  if (!root) {
    return [{ message: "service workspace must be an object", path: "$" }];
  }

  const issues: ServiceContractIssue[] = [];
  if (root.protocol !== serviceWorkspaceProtocol) {
    issues.push({
      message: `protocol must be \`${serviceWorkspaceProtocol}\``,
      path: "$.protocol",
    });
  }
  validateWorkspaceServices(root.services, issues);
  return issues;
}

export function assertServiceWorkspace(
  value: unknown
): asserts value is ServiceWorkspace {
  const issues = validateServiceWorkspace(value);
  if (issues.length > 0) {
    throw new Error(
      `Invalid Lenso service workspace: ${issues
        .map((issue) => `${issue.path} ${issue.message}`)
        .join("; ")}`
    );
  }
}

export function validateModuleRelease(value: unknown): ServiceContractIssue[] {
  const root = asRecord(value);
  if (!root) {
    return [{ message: "module release must be an object", path: "$" }];
  }

  const issues: ServiceContractIssue[] = [];
  if (root.protocol !== moduleReleaseProtocol) {
    issues.push({
      message: `protocol must be \`${moduleReleaseProtocol}\``,
      path: "$.protocol",
    });
  }
  requireNonEmptyString(root.name, "$.name", issues);
  requireNonEmptyString(root.version, "$.version", issues);
  validateModuleArtifactSource(root.source, "$.source", issues);
  if (root.source === "service") {
    validateModuleReleaseProvider(root.provider, issues);
  } else if (
    (root.source === "linked" || root.source === "bundled") &&
    root.provider !== undefined
  ) {
    validateModuleReleaseProvider(root.provider, issues);
  }
  validateStringArray(root.capabilities, "$.capabilities", issues);
  validateStringArray(root.dependencies, "$.dependencies", issues);
  return issues;
}

export function validateModuleContract(value: unknown): ServiceContractIssue[] {
  const root = asRecord(value);
  if (!root) {
    return [{ message: "module contract must be an object", path: "$" }];
  }

  const issues: ServiceContractIssue[] = [];
  if (root.protocol !== moduleContractProtocol) {
    issues.push({
      message: `protocol must be \`${moduleContractProtocol}\``,
      path: "$.protocol",
    });
  }
  requireNonEmptyString(root.name, "$.name", issues);
  requireNonEmptyString(root.version, "$.version", issues);
  validateModuleArtifactSource(root.source, "$.source", issues);
  validateStringArray(root.capabilities, "$.capabilities", issues);
  validateStringArray(root.dependencies, "$.dependencies", issues);
  if (root.manifest !== undefined && !asRecord(root.manifest)) {
    issues.push({ message: "manifest must be an object", path: "$.manifest" });
  }
  return issues;
}

export function assertModuleContract(
  value: unknown
): asserts value is ModuleContract {
  const issues = validateModuleContract(value);
  if (issues.length > 0) {
    throw new Error(
      `Invalid Lenso module contract: ${issues
        .map((issue) => `${issue.path} ${issue.message}`)
        .join("; ")}`
    );
  }
}

function validateModuleArtifactSource(
  value: unknown,
  path: string,
  issues: ServiceContractIssue[]
): void {
  if (value !== "service" && value !== "linked" && value !== "bundled") {
    issues.push({
      message: "source must be `service`, `linked`, or `bundled`",
      path,
    });
  }
}

export function assertModuleRelease(
  value: unknown
): asserts value is ModuleRelease {
  const issues = validateModuleRelease(value);
  if (issues.length > 0) {
    throw new Error(
      `Invalid Lenso module release: ${issues
        .map((issue) => `${issue.path} ${issue.message}`)
        .join("; ")}`
    );
  }
}

function validateModuleReleaseProvider(
  value: unknown,
  issues: ServiceContractIssue[]
): void {
  const provider = asRecord(value);
  if (!provider) {
    issues.push({ message: "provider must be an object", path: "$.provider" });
    return;
  }
  requireNonEmptyString(provider.name, "$.provider.name", issues);
  requireOneNonEmptyString(
    [
      provider.servicePackage,
      provider.service_package,
      provider.serviceManifest,
      provider.service_manifest,
    ],
    "$.provider.servicePackage",
    issues
  );
}

function validateProvider(
  value: unknown,
  issues: ServiceContractIssue[]
): void {
  if (value === undefined) {
    return;
  }
  const provider = asRecord(value);
  if (!provider) {
    issues.push({ message: "provider must be an object", path: "$.provider" });
    return;
  }
  requireNonEmptyString(provider.name, "$.provider.name", issues);
}

function validateCompatibility(
  value: unknown,
  issues: ServiceContractIssue[]
): void {
  if (value === undefined) {
    return;
  }
  const compatibility = asRecord(value);
  if (!compatibility) {
    issues.push({
      message: "compatibility must be an object",
      path: "$.compatibility",
    });
    return;
  }
  validateStringArray(
    compatibility.requiredHostFeatures ?? compatibility.required_host_features,
    "$.compatibility.requiredHostFeatures",
    issues
  );
}

function validateNamedFieldsArray(
  value: unknown,
  path: string,
  nameField: string,
  issues: ServiceContractIssue[]
): void {
  if (value === undefined) {
    return;
  }
  if (!Array.isArray(value)) {
    issues.push({ message: "field must be an array", path });
    return;
  }
  for (const [index, item] of value.entries()) {
    const entry = asRecord(item);
    if (!entry) {
      issues.push({
        message: "entry must be an object",
        path: `${path}[${index}]`,
      });
      continue;
    }
    requireNonEmptyString(
      entry[nameField],
      `${path}[${index}].${nameField}`,
      issues
    );
  }
}

function validateLocalProcess(
  value: unknown,
  path: string,
  issues: ServiceContractIssue[]
): void {
  if (value === undefined) {
    return;
  }
  const localProcess = asRecord(value);
  if (!localProcess) {
    issues.push({ message: "localProcess must be an object", path });
    return;
  }
  requireNonEmptyString(localProcess.command, `${path}.command`, issues);
}

function validateInstall(value: unknown, issues: ServiceContractIssue[]): void {
  if (value === undefined) {
    return;
  }
  const install = asRecord(value);
  if (!install) {
    issues.push({ message: "install must be an object", path: "$.install" });
    return;
  }
  if (install.services === undefined) {
    return;
  }
  if (!Array.isArray(install.services)) {
    issues.push({
      message: "install services must be an array",
      path: "$.install.services",
    });
    return;
  }
  for (const [index, service] of install.services.entries()) {
    const entry = asRecord(service);
    if (!entry) {
      issues.push({
        message: "service must be an object",
        path: `$.install.services[${index}]`,
      });
      continue;
    }
    requireNonEmptyString(
      entry.name,
      `$.install.services[${index}].name`,
      issues
    );
    requireNonEmptyString(
      entry.command,
      `$.install.services[${index}].command`,
      issues
    );
  }
}

function validateModules(value: unknown, issues: ServiceContractIssue[]): void {
  if (!Array.isArray(value)) {
    issues.push({ message: "modules must be an array", path: "$.modules" });
    return;
  }
  if (value.length === 0) {
    issues.push({ message: "modules must not be empty", path: "$.modules" });
    return;
  }

  const names = new Set<string>();
  for (const [index, module] of value.entries()) {
    const entry = asRecord(module);
    if (!entry) {
      issues.push({
        message: "module must be an object",
        path: `$.modules[${index}]`,
      });
      continue;
    }
    const moduleName = requireNonEmptyString(
      entry.name,
      `$.modules[${index}].name`,
      issues
    );
    if (moduleName) {
      if (names.has(moduleName)) {
        issues.push({
          message: `module \`${moduleName}\` is declared more than once`,
          path: `$.modules[${index}].name`,
        });
      }
      names.add(moduleName);
    }
    validateStringArray(
      entry.capabilities,
      `$.modules[${index}].capabilities`,
      issues
    );
    validateStringArray(
      entry.dependencies,
      `$.modules[${index}].dependencies`,
      issues
    );
  }
}

function validateServicePackageModules(
  value: unknown,
  issues: ServiceContractIssue[]
): void {
  if (!Array.isArray(value)) {
    issues.push({ message: "modules must be an array", path: "$.modules" });
    return;
  }
  if (value.length === 0) {
    issues.push({ message: "modules must not be empty", path: "$.modules" });
    return;
  }
  const names = new Set<string>();
  for (const [index, moduleNameValue] of value.entries()) {
    const moduleName = requireNonEmptyString(
      moduleNameValue,
      `$.modules[${index}]`,
      issues
    );
    if (moduleName) {
      if (names.has(moduleName)) {
        issues.push({
          message: `module \`${moduleName}\` is declared more than once`,
          path: `$.modules[${index}]`,
        });
      }
      names.add(moduleName);
    }
  }
}

function validateWorkspaceServices(
  value: unknown,
  issues: ServiceContractIssue[]
): void {
  if (value === undefined) {
    return;
  }
  if (!Array.isArray(value)) {
    issues.push({ message: "services must be an array", path: "$.services" });
    return;
  }
  const names = new Set<string>();
  for (const [index, service] of value.entries()) {
    const entry = asRecord(service);
    if (!entry) {
      issues.push({
        message: "service must be an object",
        path: `$.services[${index}]`,
      });
      continue;
    }
    const name = requireNonEmptyString(
      entry.name,
      `$.services[${index}].name`,
      issues
    );
    if (name) {
      if (names.has(name)) {
        issues.push({
          message: `service \`${name}\` is declared more than once`,
          path: `$.services[${index}].name`,
        });
      }
      names.add(name);
    }
    requireNonEmptyString(entry.lang, `$.services[${index}].lang`, issues);
    requireNonEmptyString(entry.cwd, `$.services[${index}].cwd`, issues);
    requireNonEmptyString(
      entry.manifest,
      `$.services[${index}].manifest`,
      issues
    );
    requireNonEmptyString(
      entry.command,
      `$.services[${index}].command`,
      issues
    );
    requireNonEmptyString(
      entry.readyUrl ?? entry.ready_url,
      `$.services[${index}].readyUrl`,
      issues
    );
    validateStringArray(entry.modules, `$.services[${index}].modules`, issues);
  }
}

function validateStringArray(
  value: unknown,
  path: string,
  issues: ServiceContractIssue[]
): void {
  if (value === undefined) {
    return;
  }
  if (!Array.isArray(value)) {
    issues.push({ message: "field must be an array", path });
    return;
  }
  for (const [index, item] of value.entries()) {
    requireNonEmptyString(item, `${path}[${index}]`, issues);
  }
}

function requireNonEmptyString(
  value: unknown,
  path: string,
  issues: ServiceContractIssue[]
): string | undefined {
  if (typeof value === "string" && value.trim()) {
    return value.trim();
  }
  issues.push({ message: "field must be a non-empty string", path });
  return undefined;
}

function requireOneNonEmptyString(
  values: unknown[],
  path: string,
  issues: ServiceContractIssue[]
): boolean {
  if (values.some((value) => typeof value === "string" && value.trim())) {
    return true;
  }
  issues.push({ message: "field must be a non-empty string", path });
  return false;
}

function asRecord(value: unknown): Record<string, unknown> | undefined {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    return undefined;
  }
  return value as Record<string, unknown>;
}
