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

export type ModuleHttpMethod = "GET" | "POST" | "PUT" | "PATCH" | "DELETE";

export type ModuleHttpRoute = {
  method: ModuleHttpMethod;
  path: string;
  capability?: string | null;
  display_name?: string | null;
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
  http_routes: ModuleHttpRoute[];
  admin: AdminSurface | null;
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

export type ModuleHttpRouteRow = {
  key: string;
  method: ModuleHttpMethod;
  path: string;
  capability: string;
  displayName: string;
  storyTitle: string;
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

export function moduleNavItems(
  modules: AdminModuleMetadata[]
): ModuleNavItem[] {
  return modules.flatMap((module) => {
    const schema = schemaFromModule(module);
    const surfaceKind = adminSurfaceKind(module.admin);
    const sublabel = `${module.source} / ${adminSurfaceLabel(module.admin)} / ${moduleStatusLabel(module)}`;
    const moduleItem: ModuleNavItem = {
      key: module.module_name,
      module,
      entity: null,
      label: module.module_name,
      sublabel,
      surfaceKind,
    };

    if (!schema || schema.entities.length === 0) {
      return [moduleItem] satisfies ModuleNavItem[];
    }

    return [
      moduleItem,
      ...schema.entities.map(
        (entity): ModuleNavItem => ({
          key: `${module.module_name}.${entity.name}`,
          module,
          entity,
          label: `${module.module_name} / ${entity.label}`,
          sublabel,
          surfaceKind,
        })
      ),
    ];
  });
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
