// Mirrors platform-module's admin JSON shapes. Hand-typed because the records
// and custom surface metadata are generic across arbitrary modules.

export type FieldType =
  | { kind: "string" }
  | { kind: "integer" }
  | { kind: "boolean" }
  | { kind: "timestamp" }
  | { kind: "json" };

export type FieldSchema = {
  name: string;
  label: string;
  field_type: FieldType;
  nullable: boolean;
};

export type EntitySchema = {
  name: string;
  label: string;
  fields: FieldSchema[];
  read_capability: string;
};

export type AdminSchema = { entities: EntitySchema[] };

export type ModuleSource = "linked" | "remote";
export type ModuleStatus = "loaded" | "error";
export type ModuleActivationState = "active" | "needs_attention" | "blocked";

export type ModuleHttpMethod = "GET" | "POST" | "PUT" | "PATCH" | "DELETE";

export type ModuleHttpRoute = {
  method: ModuleHttpMethod;
  path: string;
  capability?: string | null;
  display_name?: string | null;
  story_title?: string | null;
};

export type StoryDisplaySource =
  | { kind: "execution_name"; name: string }
  | { kind: "http_request"; method: string; path: string };

export type StoryDisplayDescriptor = {
  source: StoryDisplaySource;
  display_name: string;
  story_title?: string | null;
};

export type SchemaAdminSurface = AdminSchema & { kind: "schema" };

export type DeclarativeAdminSurface = {
  kind: "declarative_custom";
  pages?: DeclarativePage[];
  actions?: DeclarativeAction[];
  fallback_schema?: AdminSchema | null;
};

export type DeclarativePage = {
  name: string;
  label: string;
  sections?: DeclarativeSection[];
};

export type DeclarativeSection = {
  name: string;
  label: string;
  component: DeclarativeComponent;
};

export type DeclarativeComponent =
  | {
      kind: "metric_strip";
      metrics?: DeclarativeMetricBinding[];
    }
  | {
      kind: "entity_table";
      entity: string;
    }
  | {
      kind: "entity_detail";
      entity: string;
    };

export type DeclarativeMetricBinding = {
  label: string;
  value_path: string;
};

export type DeclarativeAction = {
  name: string;
  label: string;
  capability: string;
};

export type AdminUrlEmbeddedEntry = {
  kind: "url";
  url?: string;
  allowed_origins?: string[];
};

export type AdminEmbeddedEntry = AdminUrlEmbeddedEntry;

export type AdminSandboxPolicy = {
  allow_scripts?: boolean;
  allow_forms?: boolean;
  allow_popups?: boolean;
  allow_same_origin?: boolean;
};

export type EmbeddedAdminSurface = {
  kind: "embedded_custom";
  runtime?: string;
  entry?: AdminEmbeddedEntry;
  sandbox?: AdminSandboxPolicy;
  permissions?: unknown[];
  fallback_schema?: AdminSchema | null;
};

export type AdminSurface =
  | SchemaAdminSurface
  | DeclarativeAdminSurface
  | EmbeddedAdminSurface;

export type AdminSurfaceKind = AdminSurface["kind"] | "unavailable";

export type AdminModuleMetadata = {
  module_name: string;
  source: ModuleSource;
  status: ModuleStatus;
  error: string | null;
  source_diagnostics?: ModuleSourceDiagnostics | null;
  http_routes: ModuleHttpRoute[];
  runtime: RuntimeSurface | null;
  lifecycle: LifecycleSurface | null;
  governance: ModuleGovernance;
  manifest_lints: ModuleManifestLint[];
  story_display: StoryDisplayDescriptor[];
  capabilities: string[];
  admin: AdminSurface | null;
};

export type ModuleSourceDiagnostics = RemoteModuleSourceDiagnostics;

export type RemoteModuleSourceDiagnostics = {
  kind: "remote";
  base_url: string;
  manifest_url: string;
  timeout_ms: number;
  auth_configured: boolean;
  load_duration_ms?: number | null;
  last_checked_at?: string | null;
  last_load_error?: string | null;
};

export type AdminModuleSchemaMetadata = {
  module_name: string;
  source: ModuleSource;
  status: ModuleStatus;
  error: string | null;
  schema: AdminSchema;
};

export type AdminRecord = Record<string, unknown>;

export type ModuleNavItem = {
  key: string;
  module: AdminModuleMetadata;
  entity: EntitySchema | null;
  label: string;
  sublabel: string;
  surfaceKind: AdminSurfaceKind;
};

export type RenderedCell = {
  field: string;
  kind: FieldType["kind"];
  /** Display string for the value, already formatted per field type. */
  display: string;
};

export type DetailRow = {
  field: string;
  label: string;
  display: string;
};

export type MetadataRow = {
  label: string;
  value: string;
};

export type ModuleGovernance = {
  activation_state: ModuleActivationState;
  activation_reasons: string[];
  capability_summary: ModuleCapabilitySummary;
  capability_issues: ModuleCapabilityIssue[];
};

export type ModuleCapabilitySummary = {
  declared_count: number;
  referenced_count: number;
  missing_count: number;
  unused_count: number;
};

export type ModuleCapabilityIssue = {
  capability: string;
  subject: string;
  message: string;
  suggestion: string;
};

export type ModuleHttpRouteRow = {
  key: string;
  method: ModuleHttpMethod;
  path: string;
  capability: string;
  displayName: string;
  storyTitle: string;
};

export type RemoteModuleCallObservation = {
  success: boolean;
  error_code?: string | null;
  remote_status?: number | null;
  occurred_at: string;
};

export type RemoteModuleReadiness = {
  status: "ready" | "degraded" | "blocked";
  reasons: string[];
  latestFailure: RemoteModuleCallObservation | null;
};

export type RuntimeRetryPolicyDeclaration = {
  max_attempts: number;
  initial_delay_ms: number;
};

export type RuntimeFunctionDeclaration = {
  name: string;
  version: number;
  queue: string;
  input_schema?: string | null;
  retry_policy?: RuntimeRetryPolicyDeclaration | null;
};

export type LifecycleStartupCheck =
  | {
      kind: "function_registered";
      name: string;
      required?: boolean;
      function_name: string;
    }
  | {
      kind: "capability_declared";
      name: string;
      required?: boolean;
      capability: string;
    };

export type LifecycleActivationJob = {
  name: string;
  function_name: string;
  run_policy?: "every_startup";
  input?: unknown;
  required?: boolean;
};

export type LifecycleSurface = {
  startup_checks?: LifecycleStartupCheck[];
  activation_jobs?: LifecycleActivationJob[];
};

export type RuntimeSurface = {
  functions: RuntimeFunctionDeclaration[];
};

export type ModuleRuntimeFunctionRow = {
  key: string;
  name: string;
  version: string;
  queue: string;
  inputSchema: string;
  retryPolicy: string;
};

export type StoryDisplayRow = {
  key: string;
  source: string;
  displayName: string;
  storyTitle: string;
};

export type ModuleRegistrySourceFilter = "all" | ModuleSource;
export type ModuleRegistryStatusFilter = "all" | ModuleStatus;
export type ModuleRegistryLintFilter = "all" | ModuleLintSeverity;

export type ModuleRegistryFilters = {
  query: string;
  lint: ModuleRegistryLintFilter;
  source: ModuleRegistrySourceFilter;
  status: ModuleRegistryStatusFilter;
};

export type ModuleRegistrySummary = {
  total: number;
  linked: number;
  remote: number;
  loaded: number;
  error: number;
  lint_warning: number;
  lint_error: number;
};

export type ModuleLintSeverity = "ok" | "warning" | "error";

export type ModuleManifestLint = {
  severity: ModuleLintSeverity;
  subject: string;
  message: string;
  suggestion: string;
};

export type ModuleManifestCheck = {
  key: string;
  category: string;
  severity: ModuleLintSeverity;
  subject: string;
  message: string;
  suggestion: string;
};

export type ModuleManifestCheckGroup = {
  severity: ModuleLintSeverity;
  checks: ModuleManifestCheck[];
};

export type EmbeddedIframePolicy =
  | {
      status: "renderable";
      url: string;
      origin: string;
      sandbox: string;
    }
  | {
      status: "blocked";
      reason: string;
    };

export type DeclarativeMetric = {
  label: string;
  value: string;
};

export type DeclarativeEntitySection = {
  entity: EntitySchema | null;
  reason: string | null;
};

export function moduleStatusLabel(module: AdminModuleMetadata): ModuleStatus {
  return module.status;
}

export function moduleIsLoaded(module: AdminModuleMetadata): boolean {
  return module.status === "loaded";
}

export function moduleErrorMessage(module: AdminModuleMetadata): string | null {
  return module.status === "error"
    ? (module.error ?? "module failed to load")
    : null;
}

export function adminSurfaceLabel(surface: AdminSurface | null): string {
  if (!surface) {
    return "unavailable";
  }
  switch (surface.kind) {
    case "schema": {
      return "schema";
    }
    case "declarative_custom": {
      return "declarative custom";
    }
    case "embedded_custom": {
      return "embedded custom";
    }
    default: {
      return "custom";
    }
  }
}

export function adminSurfaceKind(
  surface: AdminSurface | null
): AdminSurfaceKind {
  return surface?.kind ?? "unavailable";
}

export function schemaFromModule(
  module: AdminModuleMetadata
): AdminSchema | null {
  return module.admin?.kind === "schema" ? module.admin : null;
}

export function schemaModulesToAdminMetadata(
  modules: AdminModuleSchemaMetadata[]
): AdminModuleMetadata[] {
  return modules.map((module) => ({
    admin: { kind: "schema", entities: module.schema.entities },
    capabilities: [],
    error: module.error,
    governance: defaultModuleGovernance(module.status),
    http_routes: [],
    manifest_lints: [],
    module_name: module.module_name,
    runtime: null,
    lifecycle: null,
    source: module.source,
    status: module.status,
    story_display: [],
  }));
}

export function moduleNavItems(
  modules: AdminModuleMetadata[]
): ModuleNavItem[] {
  return modules.flatMap((module) => {
    const schema = schemaFromModule(module);
    const surfaceKind = adminSurfaceKind(module.admin);
    const sublabel = `${module.source} / ${adminSurfaceLabel(module.admin)} / ${moduleStatusLabel(module)}`;

    if (!schema || schema.entities.length === 0) {
      return [
        {
          key: module.module_name,
          module,
          entity: null,
          label: module.module_name,
          sublabel,
          surfaceKind,
        },
      ] satisfies ModuleNavItem[];
    }

    return schema.entities.map(
      (entity): ModuleNavItem => ({
        key: `${module.module_name}.${entity.name}`,
        module,
        entity,
        label: `${module.module_name} / ${entity.label}`,
        sublabel,
        surfaceKind,
      })
    );
  });
}

export function moduleRegistrySummary(
  modules: AdminModuleMetadata[]
): ModuleRegistrySummary {
  return modules.reduce(
    (summary, module) => {
      summary.total += 1;
      summary[module.source] += 1;
      summary[module.status] += 1;
      const lintSeverity = moduleManifestHealth(module);
      if (lintSeverity === "warning") {
        summary.lint_warning += 1;
      }
      if (lintSeverity === "error") {
        summary.lint_error += 1;
      }
      return summary;
    },
    {
      error: 0,
      linked: 0,
      loaded: 0,
      remote: 0,
      lint_error: 0,
      lint_warning: 0,
      total: 0,
    }
  );
}

export function filterModuleRegistry(
  modules: AdminModuleMetadata[],
  filters: ModuleRegistryFilters
): AdminModuleMetadata[] {
  const query = filters.query.trim().toLowerCase();
  return modules.filter((module) => {
    if (filters.source !== "all" && module.source !== filters.source) {
      return false;
    }
    if (filters.status !== "all" && module.status !== filters.status) {
      return false;
    }
    if (
      filters.lint !== "all" &&
      moduleManifestHealth(module) !== filters.lint
    ) {
      return false;
    }
    if (query.length === 0) {
      return true;
    }
    return moduleRegistrySearchText(module).includes(query);
  });
}

function moduleRegistrySearchText(module: AdminModuleMetadata): string {
  const governance = moduleGovernance(module);
  const parts = [
    module.module_name,
    module.source,
    module.status,
    moduleActivationLabel(module),
    ...governance.activation_reasons,
    adminSurfaceLabel(module.admin),
    module.error ?? "",
    ...module.capabilities,
    String(governance.capability_summary.declared_count),
    String(governance.capability_summary.referenced_count),
    String(governance.capability_summary.missing_count),
    String(governance.capability_summary.unused_count),
    ...governance.capability_issues.flatMap((issue) => [
      issue.capability,
      issue.subject,
      issue.message,
      issue.suggestion,
    ]),
    ...module.http_routes.flatMap((route) => [
      route.method,
      route.path,
      route.capability ?? "",
      route.display_name ?? "",
      route.story_title ?? "",
    ]),
    ...(module.runtime?.functions ?? []).flatMap((runtimeFunction) => [
      runtimeFunction.name,
      String(runtimeFunction.version),
      runtimeFunction.queue,
      runtimeFunction.input_schema ?? "",
      retryPolicyLabel(runtimeFunction.retry_policy),
    ]),
    ...module.story_display.flatMap((descriptor) => [
      descriptor.display_name,
      descriptor.story_title ?? "",
      storyDisplaySourceLabel(descriptor.source),
    ]),
    ...moduleManifestChecks(module).flatMap((check) => [
      check.category,
      check.severity,
      check.subject,
      check.message,
      check.suggestion,
    ]),
  ];
  return parts.join(" ").toLowerCase();
}

export function moduleActivationLabel(module: AdminModuleMetadata): string {
  switch (moduleGovernance(module).activation_state) {
    case "active": {
      return "active";
    }
    case "needs_attention": {
      return "needs attention";
    }
    case "blocked": {
      return "blocked";
    }
    default: {
      return "unknown";
    }
  }
}

export function moduleGovernanceRows(
  module: AdminModuleMetadata
): MetadataRow[] {
  const governance = moduleGovernance(module);
  return [
    { label: "activation", value: moduleActivationLabel(module) },
    {
      label: "declared capabilities",
      value: String(governance.capability_summary.declared_count),
    },
    {
      label: "referenced capabilities",
      value: String(governance.capability_summary.referenced_count),
    },
    {
      label: "missing references",
      value: String(governance.capability_summary.missing_count),
    },
    {
      label: "unused declarations",
      value: String(governance.capability_summary.unused_count),
    },
  ];
}

export function remoteModuleReadiness(
  module: AdminModuleMetadata,
  recentCalls: RemoteModuleCallObservation[]
): RemoteModuleReadiness {
  const reasons: string[] = [];
  const lintHealth = moduleManifestHealth(module);
  const activation = moduleActivationLabel(module);
  const latestFailure =
    recentCalls
      .filter((call) => !call.success)
      .sort((a, b) => b.occurred_at.localeCompare(a.occurred_at))[0] ?? null;
  const failedCalls = recentCalls.filter((call) => !call.success).length;

  if (!moduleIsLoaded(module)) {
    reasons.push(moduleErrorMessage(module) ?? "module failed to load");
  }
  if (lintHealth === "error") {
    reasons.push("manifest has blocking lints");
  } else if (lintHealth === "warning") {
    reasons.push("manifest has warnings");
  }
  if (activation === "blocked") {
    reasons.push("activation is blocked");
  } else if (activation === "needs attention") {
    reasons.push("activation needs attention");
  }
  if (failedCalls > 0) {
    reasons.push(`${failedCalls}/${recentCalls.length} recent calls failed`);
  }

  if (
    !moduleIsLoaded(module) ||
    lintHealth === "error" ||
    activation === "blocked"
  ) {
    return { latestFailure, reasons, status: "blocked" };
  }
  if (reasons.length > 0) {
    return { latestFailure, reasons, status: "degraded" };
  }
  return {
    latestFailure,
    reasons: ["remote module is ready"],
    status: "ready",
  };
}

function moduleGovernance(module: AdminModuleMetadata): ModuleGovernance {
  return module.governance ?? defaultModuleGovernance(module.status);
}

function defaultModuleGovernance(status: ModuleStatus): ModuleGovernance {
  return {
    activation_state: status === "error" ? "blocked" : "active",
    activation_reasons: [],
    capability_summary: {
      declared_count: 0,
      referenced_count: 0,
      missing_count: 0,
      unused_count: 0,
    },
    capability_issues: [],
  };
}

export function adminSurfaceMetadataRows(
  module: AdminModuleMetadata
): MetadataRow[] {
  const surface = module.admin;
  if (!surface) {
    return [
      { label: "module", value: module.module_name },
      { label: "source", value: module.source },
      { label: "status", value: moduleStatusLabel(module) },
    ];
  }

  const rows: MetadataRow[] = [
    { label: "module", value: module.module_name },
    { label: "source", value: module.source },
    { label: "surface", value: adminSurfaceLabel(surface) },
    { label: "status", value: moduleStatusLabel(module) },
  ];

  if (surface.kind === "declarative_custom") {
    rows.push(
      { label: "pages", value: String(surface.pages?.length ?? 0) },
      { label: "actions", value: String(surface.actions?.length ?? 0) },
      {
        label: "fallback entities",
        value: String(surface.fallback_schema?.entities.length ?? 0),
      }
    );
  }

  if (surface.kind === "embedded_custom") {
    rows.push(
      { label: "runtime", value: surface.runtime ?? "unknown" },
      { label: "entry", value: embeddedEntryLabel(surface.entry) },
      {
        label: "allowed origins",
        value: String(
          surface.entry?.kind === "url"
            ? (surface.entry.allowed_origins?.length ?? 0)
            : 0
        ),
      },
      { label: "permissions", value: String(surface.permissions?.length ?? 0) },
      {
        label: "fallback entities",
        value: String(surface.fallback_schema?.entities.length ?? 0),
      }
    );
  }

  return rows;
}

export function moduleHttpRouteRows(
  module: AdminModuleMetadata
): ModuleHttpRouteRow[] {
  return module.http_routes.map((route, index) => ({
    capability: route.capability ?? "-",
    displayName: route.display_name ?? "-",
    key: `${route.method}:${route.path}:${index}`,
    method: route.method,
    path: route.path,
    storyTitle: route.story_title ?? "-",
  }));
}

export function moduleRuntimeFunctionRows(
  module: AdminModuleMetadata
): ModuleRuntimeFunctionRow[] {
  return (module.runtime?.functions ?? []).map((runtimeFunction, index) => ({
    inputSchema: runtimeFunction.input_schema ?? "-",
    key: `${runtimeFunction.name}:${runtimeFunction.version}:${index}`,
    name: runtimeFunction.name,
    queue: runtimeFunction.queue,
    retryPolicy: retryPolicyLabel(runtimeFunction.retry_policy),
    version: String(runtimeFunction.version),
  }));
}

export function moduleManifestChecks(
  module: AdminModuleMetadata
): ModuleManifestCheck[] {
  const routeChecks = module.manifest_lints.map(
    (lint, index): ModuleManifestCheck => ({
      category: manifestLintCategory(lint.subject),
      key: `manifest-lint:${lint.severity}:${lint.subject}:${index}`,
      message: lint.message,
      severity: lint.severity,
      subject: lint.subject,
      suggestion: lint.suggestion,
    })
  );

  if (!moduleIsLoaded(module)) {
    return [
      {
        category: "module",
        key: "module-load-error",
        message: moduleErrorMessage(module) ?? "module failed to load",
        severity: "error",
        subject: "module load",
        suggestion:
          "Refresh the module registry and inspect the module source configuration or manifest endpoint.",
      },
      ...routeChecks,
    ];
  }

  return routeChecks;
}

export function manifestLintCategory(subject: string): string {
  if (subject.startsWith("capability ") || subject.startsWith("capability.")) {
    return "capability";
  }
  if (subject === "routes" || /^[A-Z]+\s+\//u.test(subject)) {
    return "routes";
  }
  if (subject.startsWith("admin.embedded.")) {
    return "admin.embedded";
  }
  if (subject.startsWith("admin.declarative.")) {
    return "admin.declarative";
  }
  if (subject.startsWith("admin.schema")) {
    return "admin.schema";
  }
  if (subject.startsWith("runtime.")) {
    return "runtime";
  }
  if (subject === "lifecycle" || subject.startsWith("lifecycle.")) {
    return "lifecycle";
  }
  if (subject.startsWith("module.")) {
    return "module";
  }
  return "manifest";
}

function retryPolicyLabel(
  retryPolicy: RuntimeRetryPolicyDeclaration | null | undefined
): string {
  if (!retryPolicy) {
    return "-";
  }
  return `${retryPolicy.max_attempts} attempts / ${retryPolicy.initial_delay_ms}ms`;
}

export function moduleManifestHealth(
  module: AdminModuleMetadata
): ModuleLintSeverity {
  const checks = moduleManifestChecks(module);
  if (checks.some((check) => check.severity === "error")) {
    return "error";
  }
  if (checks.some((check) => check.severity === "warning")) {
    return "warning";
  }
  return "ok";
}

export function moduleManifestCheckGroups(
  checks: ModuleManifestCheck[]
): ModuleManifestCheckGroup[] {
  return (["error", "warning", "ok"] satisfies ModuleLintSeverity[])
    .map((severity) => ({
      severity,
      checks: checks.filter((check) => check.severity === severity),
    }))
    .filter((group) => group.checks.length > 0);
}

export function storyDisplayRows(
  module: AdminModuleMetadata
): StoryDisplayRow[] {
  return module.story_display.map((descriptor, index) => ({
    displayName: descriptor.display_name,
    key: `${storyDisplaySourceLabel(descriptor.source)}:${index}`,
    source: storyDisplaySourceLabel(descriptor.source),
    storyTitle: descriptor.story_title ?? "-",
  }));
}

export function storyDisplaySourceLabel(source: StoryDisplaySource): string {
  switch (source.kind) {
    case "execution_name": {
      return source.name;
    }
    case "http_request": {
      return `${source.method} ${source.path}`;
    }
    default: {
      return "unknown";
    }
  }
}

export function embeddedIframePolicy(
  surface: AdminSurface | null
): EmbeddedIframePolicy {
  if (surface?.kind !== "embedded_custom") {
    return { status: "blocked", reason: "not an embedded surface" };
  }
  if (surface.runtime !== "iframe") {
    return { status: "blocked", reason: "embedded runtime is not iframe" };
  }
  if (surface.entry?.kind !== "url" || !surface.entry.url) {
    return { status: "blocked", reason: "iframe entry URL is missing" };
  }

  const entryUrl = parseAbsoluteHttpUrl(surface.entry.url);
  if (!entryUrl) {
    return {
      status: "blocked",
      reason: "iframe entry URL must be absolute http(s)",
    };
  }

  const allowedOrigins = normalizeAllowedOrigins(
    surface.entry.allowed_origins ?? []
  );
  if (allowedOrigins.status === "invalid") {
    return { status: "blocked", reason: allowedOrigins.reason };
  }
  if (allowedOrigins.origins.length === 0) {
    return { status: "blocked", reason: "iframe origin allowlist is empty" };
  }
  if (!allowedOrigins.origins.includes(entryUrl.origin)) {
    return {
      status: "blocked",
      reason: "iframe entry origin is not allowed",
    };
  }

  return {
    status: "renderable",
    url: entryUrl.toString(),
    origin: entryUrl.origin,
    sandbox: iframeSandboxAttribute(surface.sandbox),
  };
}

export function firstDeclarativePage(
  surface: AdminSurface | null
): DeclarativePage | null {
  return surface?.kind === "declarative_custom"
    ? (surface.pages?.[0] ?? null)
    : null;
}

export function declarativeMetricValues(
  surface: DeclarativeAdminSurface,
  metrics: DeclarativeMetricBinding[]
): DeclarativeMetric[] {
  return metrics.map((metric) => ({
    label: metric.label,
    value: displayDeclarativeValue(
      resolveDeclarativePath(surface, metric.value_path)
    ),
  }));
}

export function declarativeEntitySection(
  surface: DeclarativeAdminSurface,
  entityName: string
): DeclarativeEntitySection {
  const entity =
    surface.fallback_schema?.entities.find(
      (candidate) => candidate.name === entityName
    ) ?? null;
  return entity
    ? { entity, reason: null }
    : { entity: null, reason: `fallback schema has no entity '${entityName}'` };
}

function embeddedEntryLabel(entry: AdminEmbeddedEntry | undefined): string {
  if (!entry) {
    return "unknown";
  }
  if (entry.kind === "url") {
    return entry.url ?? "url";
  }
  return entry.kind;
}

function iframeSandboxAttribute(
  sandbox: AdminSandboxPolicy | undefined
): string {
  const tokens = [
    sandbox?.allow_scripts ? "allow-scripts" : null,
    sandbox?.allow_forms ? "allow-forms" : null,
    sandbox?.allow_popups ? "allow-popups" : null,
    sandbox?.allow_same_origin ? "allow-same-origin" : null,
  ].filter((token): token is string => token !== null);
  return tokens.join(" ");
}

function parseAbsoluteHttpUrl(rawUrl: string): URL | null {
  try {
    const url = new URL(rawUrl);
    return url.protocol === "http:" || url.protocol === "https:" ? url : null;
  } catch {
    return null;
  }
}

function normalizeAllowedOrigins(
  origins: string[]
): { status: "ok"; origins: string[] } | { status: "invalid"; reason: string } {
  const normalized = new Set<string>();
  for (const origin of origins) {
    const parsed = parseAbsoluteHttpUrl(origin);
    if (!parsed) {
      return {
        status: "invalid",
        reason: "iframe allowed origin must be absolute http(s)",
      };
    }
    normalized.add(parsed.origin);
  }
  return { status: "ok", origins: [...normalized] };
}

function resolveDeclarativePath(
  surface: DeclarativeAdminSurface,
  path: string
): unknown {
  const segments = path.split(".").filter(Boolean);
  if (segments[0] !== "fallback_schema") {
    return undefined;
  }
  if (segments[1] !== "entities" || segments.length < 3) {
    return surface.fallback_schema;
  }

  const entity = surface.fallback_schema?.entities.find(
    (candidate) => candidate.name === segments[2]
  );
  if (!entity) {
    return undefined;
  }
  if (segments.length === 3) {
    return entity;
  }
  if (segments[3] === "fields" && segments[4] === "count") {
    return entity.fields.length;
  }
  if (segments[3] === "read_capability") {
    return entity.read_capability;
  }
  return undefined;
}

function displayDeclarativeValue(value: unknown): string {
  if (value === null || value === undefined) {
    return "—";
  }
  if (
    typeof value === "string" ||
    typeof value === "number" ||
    typeof value === "boolean"
  ) {
    return String(value);
  }
  return JSON.stringify(value);
}

/** Format one raw value per its field type into a display string. */
export function renderCell(field: FieldSchema, value: unknown): RenderedCell {
  const { kind } = field.field_type;
  let display: string;
  if (value === null || value === undefined) {
    display = "—";
  } else {
    switch (kind) {
      case "timestamp": {
        display = formatTimestamp(value);
        break;
      }
      case "boolean": {
        display = value ? "✓" : "✗";
        break;
      }
      case "json": {
        display = JSON.stringify(value);
        break;
      }
      default: {
        display = String(value);
      }
    }
  }
  return { field: field.name, kind, display };
}

/** Build the ordered cells for one record, driven by the entity's field schema. */
export function renderRow(
  entity: EntitySchema,
  record: AdminRecord
): RenderedCell[] {
  return entity.fields.map((field) => renderCell(field, record[field.name]));
}

export function detailRows(
  entity: EntitySchema,
  record: AdminRecord
): DetailRow[] {
  return entity.fields.map((field) => ({
    field: field.name,
    label: field.label,
    display: renderCell(field, record[field.name]).display,
  }));
}

export function recordId(record: AdminRecord): string | null {
  return typeof record.id === "string" ? record.id : null;
}

function formatTimestamp(value: unknown): string {
  const date = new Date(String(value));
  return Number.isNaN(date.getTime()) ? String(value) : date.toISOString();
}
