import { storyConsoleManifest } from "@lenso/story-console";
import { describe, expect, test } from "vitest";

import {
  adminActionDangerLevel,
  adminActionHasInput,
  adminActionInitialInputValues,
  adminActionRequiredConfirmationPhrase,
  adminActionResultSummary,
  adminSurfaceLabel,
  adminSurfaceMetadataRows,
  buildAdminActionInput,
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
  manifestLintCategory,
  moduleActivationLabel,
  moduleActivationReasons,
  moduleConsoleSurfaceRows,
  moduleDisabledByConfig,
  moduleDesiredEnabled,
  moduleEntrypointRows,
  moduleEnabledConfigKey,
  moduleErrorMessage,
  moduleGovernanceRows,
  moduleRegistryHandoffCommands,
  moduleRegistryHandoffCopyLabel,
  moduleHttpRouteRows,
  moduleIsLoaded,
  latestModuleRefreshResult,
  moduleNavItems,
  moduleRegistrySummary,
  moduleRuntimeFunctionRows,
  moduleManifestCheckGroups,
  moduleManifestChecks,
  moduleManifestHealth,
  moduleRestartPending,
  moduleRunningEnabled,
  remoteModuleReadiness,
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
  module: Omit<
    AdminModuleMetadata,
    | "capabilities"
    | "console"
    | "lifecycle"
    | "manifest_lints"
    | "governance"
    | "runtime"
    | "story_display"
  > &
    Partial<
      Pick<
        AdminModuleMetadata,
        | "capabilities"
        | "console"
        | "lifecycle"
        | "manifest_lints"
        | "governance"
        | "runtime"
        | "story_display"
      >
    >
): AdminModuleMetadata {
  return {
    capabilities: [],
    console: [],
    lifecycle: null,
    governance: {
      activation_state: "active",
      activation_reasons: [],
      capability_summary: {
        declared_count: 0,
        referenced_count: 0,
        missing_count: 0,
        unused_count: 0,
      },
      capability_issues: [],
    },
    manifest_lints: [],
    runtime: null,
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

  test("summarizes remote module readiness from metadata and recent calls", () => {
    const remoteModule = moduleMetadata({
      module_name: "remote-crm",
      source: "remote",
      status: "loaded",
      error: null,
      http_routes: [],
      admin: null,
    });

    expect(remoteModuleReadiness(remoteModule, [])).toEqual({
      latestFailure: null,
      reasons: ["remote module is ready"],
      status: "ready",
    });

    expect(
      remoteModuleReadiness(remoteModule, [
        {
          error_code: "upstream_timeout",
          occurred_at: "2026-06-03T00:00:01.000Z",
          remote_status: 504,
          success: false,
        },
        {
          occurred_at: "2026-06-03T00:00:00.000Z",
          remote_status: 200,
          success: true,
        },
      ])
    ).toMatchObject({
      latestFailure: {
        error_code: "upstream_timeout",
        remote_status: 504,
      },
      reasons: ["1/2 recent calls failed"],
      status: "degraded",
    });

    expect(remoteModuleReadiness(errorModule, [])).toMatchObject({
      reasons: [
        "remote manifest request failed",
        "manifest has blocking lints",
      ],
      status: "blocked",
    });
  });

  test("labels module activation state from governance metadata", () => {
    expect(
      moduleActivationLabel({
        ...loadedModule,
        governance: {
          activation_state: "needs_attention",
          activation_reasons: [
            "capability.reference.http_route.GET /contacts/{id}: Capability reference is not declared by the module.",
          ],
          capability_summary: {
            declared_count: 1,
            referenced_count: 1,
            missing_count: 1,
            unused_count: 0,
          },
          capability_issues: [
            {
              capability: "remote_crm.contacts.read",
              subject: "capability.reference.http_route.GET /contacts/{id}",
              message: "Capability reference is not declared by the module.",
              suggestion:
                "Add `remote_crm.contacts.read` to ModuleManifest.capabilities or update the reference.",
            },
          ],
        },
      })
    ).toBe("needs attention");
  });

  test("returns non-empty module activation reasons from governance metadata", () => {
    expect(
      moduleActivationReasons({
        ...loadedModule,
        governance: {
          activation_state: "blocked",
          activation_reasons: [
            "module failed to load: manifest unavailable",
            " ",
            "console.surface.contacts: Console route is reserved.",
          ],
          capability_summary: {
            declared_count: 0,
            referenced_count: 0,
            missing_count: 0,
            unused_count: 0,
          },
          capability_issues: [],
        },
      })
    ).toEqual([
      "module failed to load: manifest unavailable",
      "console.surface.contacts: Console route is reserved.",
    ]);
  });

  test("compares running and desired module enabled state", () => {
    const disabledModule = moduleMetadata({
      admin: null,
      error: "module disabled by configuration",
      http_routes: [],
      module_name: "identity",
      source: "linked",
      status: "error",
    });
    const values = [
      {
        key: moduleEnabledConfigKey("identity"),
        value: true,
      },
    ];

    expect(moduleRunningEnabled(loadedModule)).toBe(true);
    expect(moduleDesiredEnabled(loadedModule, [])).toBe(true);
    expect(
      moduleRestartPending(loadedModule, [
        { key: moduleEnabledConfigKey(loadedModule.module_name), value: false },
      ])
    ).toBe(true);
    expect(moduleRunningEnabled(disabledModule)).toBe(false);
    expect(moduleDisabledByConfig(disabledModule)).toBe(true);
    expect(moduleDesiredEnabled(disabledModule, values)).toBe(true);
    expect(moduleRestartPending(disabledModule, values)).toBe(true);
  });

  test("compares remote module enabled state without treating load failures as restart pending", () => {
    const disabledRemoteModule = moduleMetadata({
      admin: null,
      error: "module disabled by configuration",
      http_routes: [],
      module_name: "remote-crm",
      source: "remote",
      status: "error",
    });
    const failedRemoteModule = moduleMetadata({
      admin: null,
      error: "manifest unavailable",
      http_routes: [],
      module_name: "remote-billing",
      source: "remote",
      status: "error",
    });

    expect(
      moduleDesiredEnabled(disabledRemoteModule, [
        { key: moduleEnabledConfigKey("remote-crm"), value: true },
      ])
    ).toBe(true);
    expect(
      moduleRestartPending(disabledRemoteModule, [
        { key: moduleEnabledConfigKey("remote-crm"), value: true },
      ])
    ).toBe(true);
    expect(moduleDesiredEnabled(failedRemoteModule, [])).toBe(true);
    expect(
      moduleRestartPending(failedRemoteModule, [
        { key: moduleEnabledConfigKey("remote-billing"), value: true },
      ])
    ).toBe(false);
  });

  test("builds governance rows from backend metadata", () => {
    expect(
      moduleGovernanceRows({
        ...loadedModule,
        governance: {
          activation_state: "needs_attention",
          activation_reasons: [
            "capability.reference.http_route.GET /contacts/{id}: Capability reference is not declared by the module.",
          ],
          capability_summary: {
            declared_count: 1,
            referenced_count: 1,
            missing_count: 1,
            unused_count: 0,
          },
          capability_issues: [],
        },
      })
    ).toEqual([
      { label: "activation", value: "needs attention" },
      { label: "declared capabilities", value: "1" },
      { label: "referenced capabilities", value: "1" },
      { label: "missing references", value: "1" },
      { label: "unused declarations", value: "0" },
    ]);
  });

  test("search text includes governance capability issues", () => {
    const governanceModule = {
      ...loadedModule,
      governance: {
        activation_state: "needs_attention" as const,
        activation_reasons: [],
        capability_summary: {
          declared_count: 1,
          referenced_count: 1,
          missing_count: 1,
          unused_count: 0,
        },
        capability_issues: [
          {
            capability: "remote_crm.contacts.read",
            subject: "capability.reference.http_route.GET /contacts/{id}",
            message: "Capability reference is not declared by the module.",
            suggestion:
              "Add `remote_crm.contacts.read` to ModuleManifest.capabilities or update the reference.",
          },
        ],
      },
    };

    expect(
      filterModuleRegistry([governanceModule], {
        query: "remote_crm.contacts.read",
        lint: "all",
        source: "all",
        status: "all",
      })
    ).toHaveLength(1);
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
        proxyCommand: "curl -X GET /modules/identity/http/contacts/{id}",
        proxyPath: "/modules/identity/http/contacts/{id}",
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
        manifest_lints: [],
        console: [],
        runtime: null,
        lifecycle: null,
        governance: {
          activation_state: "active",
          activation_reasons: [],
          capability_summary: {
            declared_count: 0,
            referenced_count: 0,
            missing_count: 0,
            unused_count: 0,
          },
          capability_issues: [],
        },
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
      lint_error: 1,
      lint_warning: 0,
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
      runtime: {
        functions: [
          {
            input_schema: "remote_crm.sync_contact.v1",
            name: "remote_crm.sync_contact.v1",
            queue: "remote-crm",
            retry_policy: {
              initial_delay_ms: 1000,
              max_attempts: 3,
            },
            version: 1,
          },
        ],
      },
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
        lint: "all",
        source: "remote",
        status: "loaded",
      }).map((module) => module.module_name)
    ).toEqual(["remote-crm"]);

    expect(
      filterModuleRegistry([loadedModule, errorModule, crmModule], {
        query: "contacts.read",
        lint: "all",
        source: "all",
        status: "all",
      }).map((module) => module.module_name)
    ).toEqual(["remote-crm"]);

    expect(
      filterModuleRegistry([loadedModule, errorModule, crmModule], {
        query: "sync_contact",
        lint: "all",
        source: "all",
        status: "all",
      }).map((module) => module.module_name)
    ).toEqual(["remote-crm"]);

    expect(
      filterModuleRegistry([loadedModule, errorModule, crmModule], {
        query: "manifest request",
        lint: "all",
        source: "all",
        status: "error",
      }).map((module) => module.module_name)
    ).toEqual(["remote-crm"]);

    expect(
      filterModuleRegistry([loadedModule, errorModule, crmModule], {
        query: "",
        lint: "warning",
        source: "all",
        status: "all",
      }).map((module) => module.module_name)
    ).toEqual([]);
  });

  test("builds low-friction module install handoff commands", () => {
    expect(
      moduleRegistryHandoffCommands({
        manifestReference: "https://example.com/lenso/module/v1/manifest",
      })
    ).toEqual([
      {
        key: "add",
        label: "install",
        command:
          "lenso module add https://example.com/lenso/module/v1/manifest",
      },
      {
        key: "apply-plan",
        label: "console",
        command: "lenso console-package apply-plan",
      },
    ]);
  });

  test("labels copied module registry commands", () => {
    expect(moduleRegistryHandoffCopyLabel("inspect", "inspect")).toBe("copied");
    expect(moduleRegistryHandoffCopyLabel("install", "inspect")).toBe("copy");
  });

  test("builds module entrypoints for installed module handoff", () => {
    const module: AdminModuleMetadata = moduleMetadata({
      module_name: "remote-crm",
      source: "remote",
      status: "loaded",
      error: null,
      admin: { kind: "schema", entities: [entity] },
      console: [
        {
          area: "data",
          label: "CRM",
          name: "crm",
          package: {
            export: "CrmConsole",
            name: "@lenso/crm-console",
          },
          route: "/data/crm",
        },
      ],
      http_routes: [
        {
          capability: "remote_crm.contacts.read",
          method: "GET",
          path: "/contacts/{id}",
        },
      ],
      runtime: {
        functions: [
          {
            name: "remote_crm.sync_contact.v1",
            queue: "remote-crm",
            version: 1,
          },
        ],
      },
    });

    expect(
      moduleEntrypointRows(module, {
        hasMissingConsolePackages: true,
        restartPending: true,
      })
    ).toEqual([
      {
        detail: "restart API and worker",
        key: "restart",
        kind: "restart",
        label: "Restart Pending",
        path: "",
      },
      {
        detail: "lenso console-package apply-plan",
        key: "console-package",
        kind: "package",
        label: "Install Console Package",
        path: "",
      },
      {
        detail: "schema",
        key: "data",
        kind: "data",
        label: "Open Data",
        path: "/data",
      },
      {
        detail: "data / CRM",
        key: "console:crm",
        kind: "console",
        label: "Open Console Surface",
        path: "/data/crm",
      },
      {
        detail: "1 interface",
        key: "http",
        kind: "http",
        label: "HTTP Interfaces",
        path: "#http-interfaces",
      },
      {
        detail: "1 function",
        key: "runtime",
        kind: "runtime",
        label: "Runtime Queue",
        path: "/operations/queues?selected=runtime.functions%3Aremote-crm",
      },
    ]);
  });

  test("builds runtime function rows for registry detail", () => {
    const module: AdminModuleMetadata = moduleMetadata({
      module_name: "remote-crm",
      source: "remote",
      status: "loaded",
      error: null,
      http_routes: [],
      runtime: {
        functions: [
          {
            input_schema: "remote_crm.sync_contact.v1",
            name: "remote_crm.sync_contact.v1",
            queue: "remote-crm",
            retry_policy: {
              initial_delay_ms: 1000,
              max_attempts: 3,
            },
            version: 1,
          },
        ],
      },
      admin: null,
    });

    expect(moduleRuntimeFunctionRows(module)).toEqual([
      {
        inputSchema: "remote_crm.sync_contact.v1",
        key: "remote_crm.sync_contact.v1:1:0",
        name: "remote_crm.sync_contact.v1",
        queue: "remote-crm",
        queueKey: "runtime.functions:remote-crm",
        queuePath: "/operations/queues?selected=runtime.functions%3Aremote-crm",
        retryPolicy: "3 attempts / 1000ms",
        version: "1",
      },
    ]);
  });

  test("builds console surface rows for registry detail", () => {
    const module = moduleMetadata({
      module_name: "platform-story",
      source: "linked",
      status: "loaded",
      error: null,
      http_routes: [],
      console: [
        {
          area: storyConsoleManifest.area,
          icon: storyConsoleManifest.icon,
          label: storyConsoleManifest.label,
          name: storyConsoleManifest.surfaceName,
          package: {
            export: storyConsoleManifest.exportName,
            name: storyConsoleManifest.packageName,
          },
          navigation: storyConsoleManifest.navigation,
          required_capabilities: [...storyConsoleManifest.requiredCapabilities],
          route: storyConsoleManifest.route,
        },
      ],
      admin: null,
    });

    expect(module.console[0]?.navigation).toEqual(
      storyConsoleManifest.navigation
    );
    expect(moduleConsoleSurfaceRows(module)).toEqual([
      {
        area: "runtime",
        availability: "available",
        availabilityLabel: "available",
        availabilityReason: "host can render this console surface",
        capabilities: storyConsoleManifest.requiredCapabilities.join(", "),
        exportName: storyConsoleManifest.exportName,
        key: "stories:/runtime/stories:0",
        label: storyConsoleManifest.label,
        name: storyConsoleManifest.surfaceName,
        packageName: storyConsoleManifest.packageName,
        packageRegistration: "first_party / workspace",
        route: storyConsoleManifest.route,
      },
    ]);
    expect(
      moduleConsoleSurfaceRows(module, { availableCapabilities: [] })[0]
    ).toMatchObject({
      availability: "missing_capability",
      availabilityLabel: "missing capability",
      availabilityReason: "missing runtime.stories.read",
    });
    expect(
      moduleConsoleSurfaceRows({
        ...module,
        console: [
          {
            ...module.console[0]!,
            package: {
              export: "unknownConsoleModule",
              name: "@lenso/unknown-console",
            },
          },
        ],
      })[0]
    ).toMatchObject({
      availability: "unsupported_package",
      availabilityLabel: "unsupported package",
      availabilityReason:
        "@lenso/unknown-console#unknownConsoleModule is not registered in the host",
      packageRegistration: "not installed",
    });
  });

  test("finds the latest refresh result for a module", () => {
    const module = moduleMetadata({
      module_name: "remote-crm",
      source: "remote",
      status: "loaded",
      error: null,
      http_routes: [],
      admin: null,
    });

    expect(
      latestModuleRefreshResult(module, [
        {
          completed_at: "2026-06-03T12:00:00Z",
          duration_ms: 20,
          error: null,
          id: "refresh-old",
          module_count: 2,
          module_results: [
            {
              duration_ms: 12,
              endpoint: "http://localhost:4100/manifest",
              error: null,
              module_name: "remote-crm",
              source: "remote",
              status: "loaded",
            },
          ],
          started_at: "2026-06-03T11:59:59Z",
          status: "success",
        },
        {
          completed_at: "2026-06-03T12:05:00Z",
          duration_ms: 18,
          error: null,
          id: "refresh-new",
          module_count: 2,
          module_results: [
            {
              duration_ms: 8,
              endpoint: "http://localhost:4100/manifest",
              error: "manifest timeout",
              module_name: "remote-crm",
              source: "remote",
              status: "error",
            },
            {
              duration_ms: 3,
              endpoint: null,
              error: null,
              module_name: "identity",
              source: "linked",
              status: "loaded",
            },
          ],
          started_at: "2026-06-03T12:04:59Z",
          status: "error",
        },
      ])
    ).toEqual({
      completedAt: "2026-06-03T12:05:00Z",
      durationMs: 8,
      endpoint: "http://localhost:4100/manifest",
      error: "manifest timeout",
      recordId: "refresh-new",
      recordStatus: "error",
      status: "error",
    });
  });

  test("reports healthy route declarations", () => {
    const healthyModule = {
      ...loadedModule,
      http_routes: [
        {
          display_name: "Create User Request",
          method: "POST" as const,
          path: "/v1/identity/users",
          story_title: "User Registration",
        },
      ],
      manifest_lints: [
        {
          message: "lint ok from backend",
          severity: "ok" as const,
          subject: "routes",
          suggestion: "backend says no action",
        },
      ],
    };

    expect(moduleManifestChecks(healthyModule)).toEqual([
      {
        category: "routes",
        key: "manifest-lint:ok:routes:0",
        message: "lint ok from backend",
        severity: "ok",
        subject: "routes",
        suggestion: "backend says no action",
      },
    ]);
    expect(moduleManifestHealth(healthyModule)).toBe("ok");
  });

  test("reports route declaration quality issues", () => {
    const issueModule = moduleMetadata({
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
      manifest_lints: [
        {
          message: "lint error from backend",
          severity: "error",
          subject: "GET /contacts/{id}",
          suggestion: "backend says deduplicate",
        },
        {
          message: "lint warning display from backend",
          severity: "warning",
          subject: "GET /contacts/{id}",
          suggestion: "backend says add display",
        },
        {
          message: "lint warning story from backend",
          severity: "warning",
          subject: "GET /contacts/{id}",
          suggestion: "backend says add story",
        },
        {
          message: "lint warning capability from backend",
          severity: "warning",
          subject: "GET /contacts/{id}",
          suggestion: "backend says add capability",
        },
        {
          message: "lint warning display from backend",
          severity: "warning",
          subject: "GET /contacts/{id}",
          suggestion: "backend says add display",
        },
        {
          message: "lint warning story from backend",
          severity: "warning",
          subject: "GET /contacts/{id}",
          suggestion: "backend says add story",
        },
      ],
    });

    expect(moduleManifestChecks(issueModule)).toEqual([
      {
        category: "routes",
        key: "manifest-lint:error:GET /contacts/{id}:0",
        message: "lint error from backend",
        severity: "error",
        subject: "GET /contacts/{id}",
        suggestion: "backend says deduplicate",
      },
      {
        category: "routes",
        key: "manifest-lint:warning:GET /contacts/{id}:1",
        message: "lint warning display from backend",
        severity: "warning",
        subject: "GET /contacts/{id}",
        suggestion: "backend says add display",
      },
      {
        category: "routes",
        key: "manifest-lint:warning:GET /contacts/{id}:2",
        message: "lint warning story from backend",
        severity: "warning",
        subject: "GET /contacts/{id}",
        suggestion: "backend says add story",
      },
      {
        category: "routes",
        key: "manifest-lint:warning:GET /contacts/{id}:3",
        message: "lint warning capability from backend",
        severity: "warning",
        subject: "GET /contacts/{id}",
        suggestion: "backend says add capability",
      },
      {
        category: "routes",
        key: "manifest-lint:warning:GET /contacts/{id}:4",
        message: "lint warning display from backend",
        severity: "warning",
        subject: "GET /contacts/{id}",
        suggestion: "backend says add display",
      },
      {
        category: "routes",
        key: "manifest-lint:warning:GET /contacts/{id}:5",
        message: "lint warning story from backend",
        severity: "warning",
        subject: "GET /contacts/{id}",
        suggestion: "backend says add story",
      },
    ]);
    expect(moduleManifestHealth(issueModule)).toBe("error");
    expect(
      filterModuleRegistry([loadedModule, issueModule], {
        query: "backend says add story",
        lint: "error",
        source: "all",
        status: "all",
      }).map((module) => module.module_name)
    ).toEqual(["identity"]);
  });

  test("groups manifest lints by severity", () => {
    expect(
      moduleManifestCheckGroups([
        {
          category: "routes",
          key: "ok",
          message: "ok",
          severity: "ok",
          subject: "routes",
          suggestion: "none",
        },
        {
          category: "routes",
          key: "warning",
          message: "warning",
          severity: "warning",
          subject: "GET /contacts/{id}",
          suggestion: "fix warning",
        },
        {
          category: "routes",
          key: "error",
          message: "error",
          severity: "error",
          subject: "GET /contacts/{id}",
          suggestion: "fix error",
        },
      ]).map((group) => ({
        severity: group.severity,
        keys: group.checks.map((check) => check.key),
      }))
    ).toEqual([
      { severity: "error", keys: ["error"] },
      { severity: "warning", keys: ["warning"] },
      { severity: "ok", keys: ["ok"] },
    ]);
  });

  test("searches module-level manifest lint categories", () => {
    const module = moduleMetadata({
      ...loadedModule,
      manifest_lints: [
        {
          message: "embedded origin missing",
          severity: "warning",
          subject: "admin.embedded.entry.allowed_origins",
          suggestion: "backend says configure origins",
        },
      ],
    });

    expect(moduleManifestChecks(module)[0]).toMatchObject({
      category: "admin.embedded",
      subject: "admin.embedded.entry.allowed_origins",
    });
    expect(
      filterModuleRegistry([loadedModule, module], {
        lint: "warning",
        query: "admin.embedded",
        source: "all",
        status: "all",
      }).map((entry) => entry.module_name)
    ).toEqual(["identity"]);
  });

  test("classifies manifest lint subjects", () => {
    expect(manifestLintCategory("routes")).toBe("routes");
    expect(manifestLintCategory("GET /contacts/{id}")).toBe("routes");
    expect(manifestLintCategory("capability remote_crm.contacts.read")).toBe(
      "capability"
    );
    expect(manifestLintCategory("admin.declarative.section.contacts")).toBe(
      "admin.declarative"
    );
    expect(manifestLintCategory("admin.embedded.entry.allowed_origins")).toBe(
      "admin.embedded"
    );
    expect(manifestLintCategory("runtime.function.remote_crm.sync.v1")).toBe(
      "runtime"
    );
    expect(manifestLintCategory("lifecycle")).toBe("lifecycle");
    expect(
      manifestLintCategory("lifecycle.activation_job.warm contact cache")
    ).toBe("lifecycle");
    expect(manifestLintCategory("console.surface.stories.route")).toBe(
      "console"
    );
    expect(manifestLintCategory("module.name")).toBe("module");
  });

  test("reports module load and empty route states", () => {
    const module = moduleMetadata({
      ...errorModule,
      manifest_lints: [
        {
          message: "lint warning empty routes from backend",
          severity: "warning",
          subject: "routes",
          suggestion: "backend says add routes",
        },
      ],
    });

    expect(moduleManifestChecks(module)).toEqual([
      {
        category: "module",
        key: "module-load-error",
        message: "remote manifest request failed",
        severity: "error",
        subject: "module load",
        suggestion:
          "Refresh the module registry and inspect the module source configuration or manifest endpoint.",
      },
      {
        category: "routes",
        key: "manifest-lint:warning:routes:0",
        message: "lint warning empty routes from backend",
        severity: "warning",
        subject: "routes",
        suggestion: "backend says add routes",
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

  test("normalizes action policy metadata", () => {
    const action = {
      name: "sync_contacts",
      label: "Sync contacts",
      capability: "remote_crm.contacts.sync",
      confirmation: {
        message: "Sync remote contacts now?",
        required_phrase: " SYNC ",
      },
      input_schema: {
        fields: [
          {
            name: "dry_run",
            label: "Dry run",
            field_type: { kind: "boolean" as const },
          },
        ],
      },
    };

    expect(adminActionDangerLevel(action)).toBe("low");
    expect(adminActionHasInput(action)).toBe(true);
    expect(adminActionRequiredConfirmationPhrase(action)).toBe("SYNC");
    expect(adminActionInitialInputValues(action)).toEqual({ dry_run: false });
  });

  test("builds typed action input payloads", () => {
    const action = {
      name: "sync_contacts",
      label: "Sync contacts",
      capability: "remote_crm.contacts.sync",
      input_schema: {
        fields: [
          {
            name: "dry_run",
            label: "Dry run",
            field_type: { kind: "boolean" as const },
          },
          {
            name: "limit",
            label: "Limit",
            field_type: { kind: "integer" as const },
            required: true,
          },
          {
            name: "filter",
            label: "Filter",
            field_type: { kind: "json" as const },
          },
        ],
      },
    };

    expect(
      buildAdminActionInput(action, {
        dry_run: true,
        filter: '{"active":true}',
        limit: "25",
      })
    ).toEqual({
      error: null,
      input: { dry_run: true, filter: { active: true }, limit: 25 },
    });
  });

  test("returns the first action input validation error", () => {
    const action = {
      name: "sync_contacts",
      label: "Sync contacts",
      capability: "remote_crm.contacts.sync",
      input_schema: {
        fields: [
          {
            name: "limit",
            label: "Limit",
            field_type: { kind: "integer" as const },
            required: true,
          },
        ],
      },
    };

    expect(buildAdminActionInput(action, { limit: "" })).toEqual({
      error: "Limit is required",
      input: {},
    });
    expect(buildAdminActionInput(action, { limit: "2.5" })).toEqual({
      error: "Limit must be an integer",
      input: {},
    });
  });

  test("summarizes action results for operator feedback", () => {
    expect(
      adminActionResultSummary({
        contacts: 3,
        dry_run: true,
        nested: { queued: false },
        skipped: null,
        synced: true,
      })
    ).toBe(
      'contacts: 3 / dry_run: true / nested: {"queued":false} / skipped: —'
    );
    expect(adminActionResultSummary([1, 2, 3])).toBe("3 items");
    expect(adminActionResultSummary(null)).toBe("no result");
    expect(adminActionResultSummary("ok")).toBe("ok");
    expect(adminActionResultSummary("x".repeat(120))).toHaveLength(96);
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
