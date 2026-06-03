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

export type SchemaAdminSurface = AdminSchema & { kind: "schema" };

export type DeclarativeAdminSurface = {
  kind: "declarative_custom";
  pages?: unknown[];
  actions?: unknown[];
  fallback_schema?: AdminSchema | null;
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
