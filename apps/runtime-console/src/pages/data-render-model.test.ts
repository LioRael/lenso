import { describe, expect, test } from "vitest";

import {
  adminSurfaceLabel,
  adminSurfaceMetadataRows,
  declarativeEntitySection,
  declarativeMetricValues,
  detailRows,
  embeddedIframePolicy,
  filterModuleRegistry,
  type AdminModuleMetadata,
  type DeclarativeAdminSurface,
  type EntitySchema,
  type FieldSchema,
  firstDeclarativePage,
  moduleErrorMessage,
  moduleHttpRouteRows,
  moduleIsLoaded,
  moduleNavItems,
  moduleRegistrySummary,
  moduleRouteChecks,
  moduleStatusLabel,
  recordId,
  renderCell,
  renderRow,
  schemaModulesToAdminMetadata,
  storyDisplayRows,
} from "./data-render-model";

const emailField: FieldSchema = {
  name: "email",
  label: "Email",
  field_type: { kind: "string" },
  nullable: false,
};
const activeField: FieldSchema = {
  name: "active",
  label: "Active",
  field_type: { kind: "boolean" },
  nullable: false,
};
const createdAtField: FieldSchema = {
  name: "created_at",
  label: "Created",
  field_type: { kind: "timestamp" },
  nullable: false,
};
const metaField: FieldSchema = {
  name: "meta",
  label: "Meta",
  field_type: { kind: "json" },
  nullable: true,
};

const entity: EntitySchema = {
  name: "users",
  label: "Users",
  read_capability: "identity.users.read",
  fields: [emailField, activeField, createdAtField, metaField],
};

function moduleMetadata(
  module: Omit<AdminModuleMetadata, "capabilities" | "story_display"> &
    Partial<Pick<AdminModuleMetadata, "capabilities" | "story_display">>
): AdminModuleMetadata {
  return {
    capabilities: [],
    story_display: [],
    ...module,
  };
}

describe("renderCell", () => {
  test("renders strings verbatim", () => {
    expect(renderCell(emailField, "a@example.com").display).toBe(
      "a@example.com"
    );
  });
  test("renders booleans as check/cross", () => {
    expect(renderCell(activeField, true).display).toBe("✓");
    expect(renderCell(activeField, false).display).toBe("✗");
  });
  test("renders timestamps as ISO", () => {
    expect(renderCell(createdAtField, "2026-06-03T00:00:00Z").display).toBe(
      "2026-06-03T00:00:00.000Z"
    );
  });
  test("stringifies json", () => {
    expect(renderCell(metaField, { a: 1 }).display).toBe('{"a":1}');
  });
  test("renders null/absent as em dash", () => {
    expect(renderCell(emailField, null).display).toBe("—");
    expect(renderCell(metaField, undefined).display).toBe("—");
  });
  test("renders integers as string", () => {
    const intField: FieldSchema = {
      name: "count",
      label: "Count",
      field_type: { kind: "integer" },
      nullable: false,
    };
    expect(renderCell(intField, 42).display).toBe("42");
  });
});

describe("renderRow", () => {
  test("produces one cell per schema field, in order", () => {
    const cells = renderRow(entity, {
      email: "a@example.com",
      active: true,
      created_at: "2026-06-03T00:00:00Z",
      meta: { x: 1 },
    });
    expect(cells.map((c) => c.field)).toEqual([
      "email",
      "active",
      "created_at",
      "meta",
    ]);
  });
});

describe("recordId", () => {
  test("uses a string id when present", () => {
    expect(recordId({ id: "contact_1", email: "a@example.com" })).toBe(
      "contact_1"
    );
  });

  test("returns null when the record has no string id", () => {
    expect(recordId({ email: "a@example.com" })).toBeNull();
    expect(recordId({ id: 42 })).toBeNull();
  });
});

describe("detailRows", () => {
  test("renders detail values from schema fields in order", () => {
    const rows = detailRows(entity, {
      email: "a@example.com",
      active: true,
      created_at: "2026-06-03T00:00:00Z",
      meta: { x: 1 },
    });

    expect(rows).toEqual([
      { field: "email", label: "Email", display: "a@example.com" },
      { field: "active", label: "Active", display: "✓" },
      {
        field: "created_at",
        label: "Created",
        display: "2026-06-03T00:00:00.000Z",
      },
      { field: "meta", label: "Meta", display: '{"x":1}' },
    ]);
  });
});

describe("moduleStatusLabel", () => {
  const moduleSchema: AdminModuleMetadata = moduleMetadata({
    module_name: "remote-crm",
    source: "remote",
    status: "loaded",
    error: null,
    http_routes: [],
    admin: { kind: "schema", entities: [] },
  });

  test("uses backend loaded status verbatim", () => {
    expect(moduleStatusLabel(moduleSchema)).toBe("loaded");
  });

  test("maps backend error status objects to error", () => {
    expect(
      moduleStatusLabel({
        ...moduleSchema,
        status: "error",
        error: "manifest failed",
      })
    ).toBe("error");
  });
});

describe("module status helpers", () => {
  const loadedModule: AdminModuleMetadata = moduleMetadata({
    module_name: "identity",
    source: "linked",
    status: "loaded",
    error: null,
    http_routes: [],
    admin: { kind: "schema", entities: [entity] },
  });
  const errorModule: AdminModuleMetadata = moduleMetadata({
    module_name: "remote-crm",
    source: "remote",
    status: "error",
    error: "remote manifest request failed",
    http_routes: [],
    admin: null,
  });

  test("identifies loaded modules", () => {
    expect(moduleIsLoaded(loadedModule)).toBe(true);
    expect(moduleIsLoaded(errorModule)).toBe(false);
  });

  test("returns the backend error message for failed modules", () => {
    expect(moduleErrorMessage(errorModule)).toBe(
      "remote manifest request failed"
    );
    expect(moduleErrorMessage(loadedModule)).toBeNull();
  });

  test("builds remote HTTP route rows with story display metadata", () => {
    expect(
      moduleHttpRouteRows({
        ...loadedModule,
        http_routes: [
          {
            capability: "remote_crm.contacts.read",
            display_name: "Fetch Contact",
            method: "GET",
            path: "/contacts/{id}",
            story_title: "Fetch Contact",
          },
        ],
      })
    ).toEqual([
      {
        capability: "remote_crm.contacts.read",
        displayName: "Fetch Contact",
        key: "GET:/contacts/{id}:0",
        method: "GET",
        path: "/contacts/{id}",
        storyTitle: "Fetch Contact",
      },
    ]);
  });

  test("builds story display rows from runtime descriptor sources", () => {
    expect(
      storyDisplayRows({
        ...loadedModule,
        story_display: [
          {
            display_name: "Create User Request",
            source: {
              kind: "http_request",
              method: "POST",
              path: "/v1/identity/users",
            },
            story_title: "User Registration",
          },
          {
            display_name: "Send Welcome Email",
            source: {
              kind: "execution_name",
              name: "notifications.send_welcome_email.v1",
            },
          },
        ],
      })
    ).toEqual([
      {
        displayName: "Create User Request",
        key: "POST /v1/identity/users:0",
        source: "POST /v1/identity/users",
        storyTitle: "User Registration",
      },
      {
        displayName: "Send Welcome Email",
        key: "notifications.send_welcome_email.v1:1",
        source: "notifications.send_welcome_email.v1",
        storyTitle: "-",
      },
    ]);
  });

  test("keeps failed empty-schema modules visible in nav", () => {
    expect(moduleNavItems([loadedModule, errorModule])).toEqual([
      {
        key: "identity.users",
        module: loadedModule,
        entity,
        label: "identity / Users",
        sublabel: "linked / schema / loaded",
        surfaceKind: "schema",
      },
      {
        key: "remote-crm",
        module: errorModule,
        entity: null,
        label: "remote-crm",
        sublabel: "remote / unavailable / error",
        surfaceKind: "unavailable",
      },
    ]);
  });

  test("keeps custom surfaces visible without schema entities", () => {
    const declarativeModule: AdminModuleMetadata = moduleMetadata({
      module_name: "billing",
      source: "linked",
      status: "loaded",
      error: null,
      http_routes: [],
      admin: {
        kind: "declarative_custom",
        pages: [{ name: "overview", label: "Overview" }],
        actions: [],
        fallback_schema: { entities: [] },
      },
    });
    const embeddedModule: AdminModuleMetadata = moduleMetadata({
      module_name: "remote-crm-embedded",
      source: "remote",
      status: "loaded",
      error: null,
      http_routes: [],
      admin: {
        kind: "embedded_custom",
        runtime: "iframe",
        entry: {
          kind: "url",
          url: "https://remote-crm.example.test/admin",
          allowed_origins: ["https://remote-crm.example.test"],
        },
        sandbox: { allow_scripts: true },
        permissions: [],
        fallback_schema: { entities: [entity] },
      },
    });

    expect(moduleNavItems([declarativeModule, embeddedModule])).toEqual([
      {
        key: "billing",
        module: declarativeModule,
        entity: null,
        label: "billing",
        sublabel: "linked / declarative custom / loaded",
        surfaceKind: "declarative_custom",
      },
      {
        key: "remote-crm-embedded",
        module: embeddedModule,
        entity: null,
        label: "remote-crm-embedded",
        sublabel: "remote / embedded custom / loaded",
        surfaceKind: "embedded_custom",
      },
    ]);
  });

  test("converts schema endpoint modules into data-page metadata", () => {
    expect(
      schemaModulesToAdminMetadata([
        {
          module_name: "identity",
          source: "linked",
          status: "loaded",
          error: null,
          schema: { entities: [entity] },
        },
      ])
    ).toEqual([
      {
        module_name: "identity",
        source: "linked",
        status: "loaded",
        error: null,
        http_routes: [],
        story_display: [],
        capabilities: [],
        admin: { kind: "schema", entities: [entity] },
      },
    ]);
  });

  test("summarizes module registry source and status counts", () => {
    expect(moduleRegistrySummary([loadedModule, errorModule])).toEqual({
      error: 1,
      linked: 1,
      loaded: 1,
      remote: 1,
      total: 2,
    });
  });

  test("filters module registry by source, status, and search text", () => {
    const crmModule: AdminModuleMetadata = moduleMetadata({
      module_name: "remote-crm",
      source: "remote",
      status: "loaded",
      error: null,
      capabilities: ["remote_crm.contacts.read"],
      http_routes: [
        {
          capability: "remote_crm.contacts.read",
          display_name: "Fetch Contact",
          method: "GET",
          path: "/contacts/{id}",
          story_title: "Fetch Contact",
        },
      ],
      story_display: [
        {
          display_name: "Fetch Contact",
          source: {
            kind: "http_request",
            method: "GET",
            path: "/modules/remote-crm/http/contacts/{id}",
          },
          story_title: "CRM Contact Lookup",
        },
      ],
      admin: null,
    });

    expect(
      filterModuleRegistry([loadedModule, errorModule, crmModule], {
        query: "",
        source: "remote",
        status: "loaded",
      }).map((module) => module.module_name)
    ).toEqual(["remote-crm"]);

    expect(
      filterModuleRegistry([loadedModule, errorModule, crmModule], {
        query: "contacts.read",
        source: "all",
        status: "all",
      }).map((module) => module.module_name)
    ).toEqual(["remote-crm"]);

    expect(
      filterModuleRegistry([loadedModule, errorModule, crmModule], {
        query: "manifest request",
        source: "all",
        status: "error",
      }).map((module) => module.module_name)
    ).toEqual(["remote-crm"]);
  });

  test("reports healthy route declarations", () => {
    expect(
      moduleRouteChecks({
        ...loadedModule,
        http_routes: [
          {
            display_name: "Create User Request",
            method: "POST",
            path: "/v1/identity/users",
            story_title: "User Registration",
          },
        ],
      })
    ).toEqual([
      {
        key: "routes-complete",
        message: "Declared routes include display and story metadata.",
        severity: "ok",
        subject: "routes",
      },
    ]);
  });

  test("reports route declaration quality issues", () => {
    expect(
      moduleRouteChecks({
        ...loadedModule,
        source: "remote",
        http_routes: [
          {
            method: "GET",
            path: "/contacts/{id}",
          },
          {
            capability: "remote_crm.contacts.read",
            method: "GET",
            path: "/contacts/{id}",
          },
        ],
      })
    ).toEqual([
      {
        key: "duplicate:GET /contacts/{id}",
        message: "2 routes declare the same method and path.",
        severity: "error",
        subject: "GET /contacts/{id}",
      },
      {
        key: "display:GET /contacts/{id}:0",
        message: "Missing display_name for compact runtime story nodes.",
        severity: "warning",
        subject: "GET /contacts/{id}",
      },
      {
        key: "story:GET /contacts/{id}:0",
        message: "Missing story_title for direct HTTP entry stories.",
        severity: "warning",
        subject: "GET /contacts/{id}",
      },
      {
        key: "capability:GET /contacts/{id}:0",
        message: "Missing capability declaration for host proxy authorization.",
        severity: "warning",
        subject: "GET /contacts/{id}",
      },
      {
        key: "display:GET /contacts/{id}:1",
        message: "Missing display_name for compact runtime story nodes.",
        severity: "warning",
        subject: "GET /contacts/{id}",
      },
      {
        key: "story:GET /contacts/{id}:1",
        message: "Missing story_title for direct HTTP entry stories.",
        severity: "warning",
        subject: "GET /contacts/{id}",
      },
    ]);
  });

  test("reports module load and empty route states", () => {
    expect(moduleRouteChecks(errorModule)).toEqual([
      {
        key: "module-load-error",
        message: "remote manifest request failed",
        severity: "error",
        subject: "module load",
      },
      {
        key: "no-routes",
        message: "No HTTP interfaces are declared in this manifest.",
        severity: "warning",
        subject: "routes",
      },
    ]);
  });
});

describe("admin surface metadata helpers", () => {
  test("labels known surfaces", () => {
    expect(adminSurfaceLabel(null)).toBe("unavailable");
    expect(adminSurfaceLabel({ kind: "schema", entities: [] })).toBe("schema");
    expect(adminSurfaceLabel({ kind: "declarative_custom" })).toBe(
      "declarative custom"
    );
    expect(adminSurfaceLabel({ kind: "embedded_custom" })).toBe(
      "embedded custom"
    );
  });

  test("summarizes embedded surfaces without exposing executable code", () => {
    const module: AdminModuleMetadata = moduleMetadata({
      module_name: "remote-crm-embedded",
      source: "remote",
      status: "loaded",
      error: null,
      http_routes: [],
      admin: {
        kind: "embedded_custom",
        runtime: "iframe",
        entry: {
          kind: "url",
          url: "https://remote-crm.example.test/admin",
          allowed_origins: ["https://remote-crm.example.test"],
        },
        permissions: [{ kind: "read", capability: "remote_crm.contacts.read" }],
        fallback_schema: { entities: [entity] },
      },
    });

    expect(adminSurfaceMetadataRows(module)).toEqual([
      { label: "module", value: "remote-crm-embedded" },
      { label: "source", value: "remote" },
      { label: "surface", value: "embedded custom" },
      { label: "status", value: "loaded" },
      { label: "runtime", value: "iframe" },
      { label: "entry", value: "https://remote-crm.example.test/admin" },
      { label: "allowed origins", value: "1" },
      { label: "permissions", value: "1" },
      { label: "fallback entities", value: "1" },
    ]);
  });
});

describe("declarative admin helpers", () => {
  const surface: DeclarativeAdminSurface = {
    kind: "declarative_custom",
    pages: [
      {
        name: "overview",
        label: "Overview",
        sections: [
          {
            name: "health",
            label: "Health",
            component: {
              kind: "metric_strip",
              metrics: [
                {
                  label: "Fields",
                  value_path: "fallback_schema.entities.users.fields.count",
                },
                {
                  label: "Capability",
                  value_path: "fallback_schema.entities.users.read_capability",
                },
              ],
            },
          },
        ],
      },
    ],
    actions: [],
    fallback_schema: { entities: [entity] },
  };

  test("uses the first declared page", () => {
    expect(firstDeclarativePage(surface)?.name).toBe("overview");
    expect(firstDeclarativePage({ kind: "schema", entities: [] })).toBeNull();
  });

  test("resolves metric bindings from fallback schema data", () => {
    expect(
      declarativeMetricValues(surface, [
        {
          label: "Fields",
          value_path: "fallback_schema.entities.users.fields.count",
        },
        {
          label: "Capability",
          value_path: "fallback_schema.entities.users.read_capability",
        },
        {
          label: "Missing",
          value_path: "fallback_schema.entities.widgets.fields.count",
        },
      ])
    ).toEqual([
      { label: "Fields", value: "4" },
      { label: "Capability", value: "identity.users.read" },
      { label: "Missing", value: "—" },
    ]);
  });

  test("looks up entity table declarations in fallback schema", () => {
    expect(declarativeEntitySection(surface, "users")).toEqual({
      entity,
      reason: null,
    });
    expect(declarativeEntitySection(surface, "widgets")).toEqual({
      entity: null,
      reason: "fallback schema has no entity 'widgets'",
    });
  });
});

describe("embeddedIframePolicy", () => {
  test("renders an iframe only for allowed absolute http origins", () => {
    expect(
      embeddedIframePolicy({
        kind: "embedded_custom",
        runtime: "iframe",
        entry: {
          kind: "url",
          url: "https://remote-crm.example.test/admin?tenant=demo",
          allowed_origins: ["https://remote-crm.example.test"],
        },
        sandbox: {
          allow_scripts: true,
          allow_forms: true,
          allow_popups: false,
          allow_same_origin: false,
        },
      })
    ).toEqual({
      status: "renderable",
      url: "https://remote-crm.example.test/admin?tenant=demo",
      origin: "https://remote-crm.example.test",
      sandbox: "allow-scripts allow-forms",
    });
  });

  test("blocks iframe URLs outside the declared origin allowlist", () => {
    expect(
      embeddedIframePolicy({
        kind: "embedded_custom",
        runtime: "iframe",
        entry: {
          kind: "url",
          url: "https://evil.example.test/admin",
          allowed_origins: ["https://remote-crm.example.test"],
        },
      })
    ).toEqual({
      status: "blocked",
      reason: "iframe entry origin is not allowed",
    });
  });

  test("blocks non-http iframe URLs and invalid allowed origins", () => {
    expect(
      embeddedIframePolicy({
        kind: "embedded_custom",
        runtime: "iframe",
        entry: {
          kind: "url",
          url: "ftp://remote-crm.example.test/admin",
          allowed_origins: ["https://remote-crm.example.test"],
        },
      })
    ).toEqual({
      status: "blocked",
      reason: "iframe entry URL must be absolute http(s)",
    });

    expect(
      embeddedIframePolicy({
        kind: "embedded_custom",
        runtime: "iframe",
        entry: {
          kind: "url",
          url: "https://remote-crm.example.test/admin",
          allowed_origins: ["not-an-origin"],
        },
      })
    ).toEqual({
      status: "blocked",
      reason: "iframe allowed origin must be absolute http(s)",
    });
  });

  test("blocks reserved embedded runtimes until they have host policies", () => {
    expect(
      embeddedIframePolicy({
        kind: "embedded_custom",
        runtime: "wasm",
        entry: {
          kind: "url",
          url: "https://remote-crm.example.test/admin",
          allowed_origins: ["https://remote-crm.example.test"],
        },
      })
    ).toEqual({
      status: "blocked",
      reason: "embedded runtime is not iframe",
    });
  });
});
