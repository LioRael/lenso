import { createHash } from "node:crypto";
import { once } from "node:events";
/* eslint-disable func-style, no-use-before-define */
import type { IncomingMessage } from "node:http";
import { connect } from "node:http2";

import { describe, expect, test } from "vitest";

import {
  actionBooleanField,
  actionConfirmation,
  adminAction,
  adminSchema,
  booleanField,
  createMemoryProviderInvocationStore,
  declarativeCustom,
  declarativePage,
  declarativeSection,
  defineModule,
  defineProviderModule,
  defineService,
  defineSchemaEntity,
  entityTable,
  eventHandler,
  everyStartup,
  getRoute,
  integerField,
  jsonField,
  lifecycle,
  metricBinding,
  metricStrip,
  postRoute,
  problemDetails,
  providerFailed,
  providerPending,
  providerRejected,
  providerSucceeded,
  providerV1OutcomeLimits,
  queryValue,
  readLensoInvocationContext,
  runtimeFunction,
  serveModuleProviderGrpc,
  schemaAdmin,
  serveModuleProvider,
  serveService,
  textField,
  timestampField,
  verifyProviderInvocationStoreConformance,
} from "./service-module-delivery";
import type {
  ModuleAdminDataSource,
  ModuleEventHandler,
  ProviderInvocationStore,
  ProviderV1Options,
} from "./service-module-delivery";

const providerTestDigest = (character: string) =>
  `sha256:${character.repeat(64)}`;

const providerTestFixture = () => {
  const manifest = {
    capabilities: ["taste.profiles.read"],
    module_id: "taste/profile",
    protocol: "lenso.module-manifest.v1",
    runtime: {
      functions: [{ name: "taste.confirm-receipt.v1" }],
    },
  };
  const manifestDigest = `sha256:${createHash("sha256")
    .update(JSON.stringify(manifest))
    .digest("hex")}`;
  const moduleRelease = {
    compatibility: {},
    delivery: {
      contract_digests: [providerTestDigest("4")],
      export: "taste-profile",
      kind: "service",
      responsibility_profile: "provider",
      service_id: "taste/service",
      service_release_digest: providerTestDigest("5"),
      service_release_version: "0.1.0",
    },
    manifest,
    manifest_digest: manifestDigest,
    module_id: "taste/profile",
    protocol: "lenso.module-release.v1",
    version: "0.1.0",
  };
  const moduleReleaseDigest = `sha256:${createHash("sha256")
    .update(JSON.stringify(moduleRelease))
    .digest("hex")}`;
  const service = defineService({
    modules: [
      defineModule({
        httpRoutes: [getRoute("/profiles/{id}")],
        name: "taste-profile",
      }),
    ],
    name: "taste-service",
  });
  const providerV1 = {
    exports: [
      {
        contractDigests: { http: providerTestDigest("4") },
        exportKey: "taste-profile",
        manifest,
        manifestDigest,
        moduleId: "taste/profile",
        moduleReleaseDigest,
        moduleVersion: "0.1.0",
      },
    ],
    moduleReleases: { "taste-profile": moduleRelease },
    protocolContractDigest: providerTestDigest("1"),
    runtimeInstanceId: "taste-local-1",
    serviceId: "taste/service",
    serviceReleaseDigest: providerTestDigest("5"),
    serviceReleaseVersion: "0.1.0",
  } satisfies ProviderV1Options;
  const invocation = {
    actor: {
      kind: "user" as const,
      scopes: ["taste.profiles.read"],
      user_id: "taste-user",
    },
    attempt: 1,
    contentType: "application/json",
    correlationId: "correlation-1",
    deadline: "2026-08-13T01:00:00Z",
    exportKey: "taste-profile",
    inputContractDigest: providerTestDigest("4"),
    invocationId: "invocation-1",
    manifestDigest,
    moduleReleaseDigest,
    operationKind: "http_route",
    operationName: "GET /profiles/{id}",
    operationVersion: "1",
    outputContractDigest: providerTestDigest("4"),
    payload: {
      declared_path: "/profiles/{id}",
      method: "GET",
      path_params: { id: "profile-1" },
    },
    protocol: "lenso.provider.v1" as const,
    requestId: "request-1",
    serviceReleaseDigest: providerTestDigest("5"),
    mode: "durable" as const,
    trace: { baggage: [], span_id: null, trace_id: null },
  };
  return {
    invocation,
    manifest,
    manifestDigest,
    moduleRelease,
    moduleReleaseDigest,
    providerV1,
    service,
  };
};

const durableProviderTestStore = (
  inner = createMemoryProviderInvocationStore()
): ProviderInvocationStore => {
  return {
    acknowledge: (input) => inner.acknowledge(input),
    claim: (input) => inner.claim(input),
    complete: (input) => inner.complete(input),
    durability: "durable",
    get: (invocationId) => inner.get(invocationId),
  };
};

describe("@lenso/service-kit internal delivery adapter", () => {
  test("publishes a reusable durable invocation Store conformance vector", async () => {
    const backingStore = createMemoryProviderInvocationStore();
    const result = await verifyProviderInvocationStoreConformance({
      createStore: () => durableProviderTestStore(backingStore),
    });
    expect(result).toMatchObject({
      invocationId: expect.stringContaining("provider-store-conformance:"),
      outcomeDigest: expect.stringMatching(/^sha256:[0-9a-f]{64}$/u),
    });
    await expect(backingStore.get(result.invocationId)).resolves.toMatchObject({
      acknowledgedOutcomeDigest: result.outcomeDigest,
      outcome: {
        effectEvidence: [
          { kind: "remote_receipt", receiptId: "conformance-failed" },
        ],
        error: {
          providerTraceReference: "conformance-failed-trace",
          retryAfterMs: 2_500,
          retryable: true,
        },
        status: "failed",
      },
      phase: "completed",
    });
    await expect(
      backingStore.get(`${result.invocationId}:rejected`)
    ).resolves.toMatchObject({
      outcome: {
        effectEvidence: [
          { kind: "remote_receipt", receiptId: "conformance-rejected" },
        ],
        error: {
          providerTraceReference: "conformance-rejected-trace",
          retryAfterMs: null,
          retryable: false,
        },
        status: "rejected",
      },
      phase: "completed",
    });
    await expect(
      verifyProviderInvocationStoreConformance({
        createStore: createMemoryProviderInvocationStore,
      })
    ).rejects.toThrow("requires durable storage");

    const singleton = durableProviderTestStore();
    await expect(
      verifyProviderInvocationStoreConformance({
        createStore: () => singleton,
      })
    ).rejects.toThrow("requires fresh adapter instances");
  });

  test("preserves legacy handler result types while accepting Provider-aware handlers", async () => {
    const readLegacyRecords = async (source: ModuleAdminDataSource) => {
      const page = await source.list({ limit: 1 });
      return page.records;
    };
    const readLegacyActions = async (
      handler: ModuleEventHandler,
      context: Parameters<ModuleEventHandler>[0]
    ) => {
      const result = await handler(context);
      return result?.actions;
    };
    expect(readLegacyRecords).toBeTypeOf("function");
    expect(readLegacyActions).toBeTypeOf("function");

    const fixture = providerTestFixture();
    const served = await serveService(fixture.service, {
      modules: {
        "taste-profile": {
          data: {
            profiles: {
              detail: () => null,
              list: () =>
                providerFailed({
                  code: "provider_unavailable",
                  message: "The Provider is unavailable",
                }),
            },
          },
          events: {
            "taste.profile-observed.v1": () => providerPending(),
          },
        },
      },
      port: 0,
      providerV1: fixture.providerV1,
    });
    await served.close();
  });

  test("builds the canonical top-level Problem Details contract", () => {
    expect(
      problemDetails({
        code: "validation",
        detail: "Ticket title is required",
        errors: [{ field: "title", reason: "required" }],
        nextActions: ["Provide a non-empty title"],
        status: 400,
      })
    ).toEqual({
      body: {
        code: "validation",
        correlation_id: null,
        detail: "Ticket title is required",
        errors: [{ field: "title", reason: "required" }],
        next_actions: ["Provide a non-empty title"],
        request_id: null,
        status: 400,
        title: "Validation failed",
        type: "https://lenso.dev/problems/validation",
      },
      statusCode: 400,
    });
  });

  test("defines a serializable service module manifest", () => {
    expect(
      defineProviderModule({
        capabilities: ["billing.read"],
        console: [
          {
            area: "data",
            label: "Billing",
            name: "billing",
            package: {
              export: "billingConsoleModule",
              name: "@vendor/lenso-billing-console",
            },
            required_capabilities: ["billing.read"],
            route: "/data/billing",
          },
        ],
        name: "billing",
      })
    ).toEqual({
      admin: null,
      capabilities: ["billing.read"],
      console: [
        {
          area: "data",
          label: "Billing",
          name: "billing",
          package: {
            export: "billingConsoleModule",
            name: "@vendor/lenso-billing-console",
          },
          required_capabilities: ["billing.read"],
          route: "/data/billing",
        },
      ],
      dependencies: [],
      http_routes: [],
      name: "billing",
      runtime: {
        functions: [],
      },
      source: "service",
      story_display: [],
      version: "0.1.0",
    });
  });

  test("defines event handler declarations and dependency metadata", () => {
    expect(
      defineProviderModule({
        dependencies: ["identity"],
        eventHandlers: [
          eventHandler(
            "sync_contact_on_user_registered",
            "identity.user_registered.v1"
          ),
        ],
        name: "crm",
        storyDisplay: [
          {
            display_name: "Fetch Contact",
            source: {
              kind: "http_request",
              method: "GET",
              path: "/contacts/{id}",
            },
            story_title: "Fetch Contact",
          },
        ],
      })
    ).toMatchObject({
      dependencies: ["identity"],
      events: {
        handlers: [
          {
            event_name: "identity.user_registered.v1",
            name: "sync_contact_on_user_registered",
          },
        ],
      },
      story_display: [
        {
          display_name: "Fetch Contact",
          source: {
            kind: "http_request",
            method: "GET",
            path: "/contacts/{id}",
          },
          story_title: "Fetch Contact",
        },
      ],
    });
  });

  test("defines service release metadata", () => {
    expect(
      defineProviderModule({
        compatibility: {
          console_package_api: "1",
          required_host_features: ["service.status"],
        },
        name: "billing",
        service: {
          deployment: {
            commands: ["docker compose up billing"],
            target: "container",
          },
          name: "api",
          required_env: ["BILLING_DATABASE_URL"],
          status_path: "/lenso/module/v1/status",
          transports: ["http"],
        },
      })
    ).toMatchObject({
      compatibility: {
        console_package_api: "1",
      },
      service: {
        name: "api",
        required_env: ["BILLING_DATABASE_URL"],
        status_path: "/lenso/module/v1/status",
      },
    });
  });

  test("defines a service manifest with provided modules", () => {
    const supportTicket = defineModule({
      capabilities: ["support_ticket.tickets.read"],
      dependencies: ["lenso/identity"],
      httpRoutes: [
        getRoute("/tickets/{id}", {
          capability: "support_ticket.tickets.read",
          displayName: "Get Ticket",
          storyTitle: "Get Ticket",
        }),
      ],
      name: "acme/support-ticket",
    });

    expect(supportTicket).toMatchObject({
      capabilities: ["support_ticket.tickets.read"],
      module_id: "acme/support-ticket",
      protocol: "lenso.module-manifest.v1",
      requires: [
        {
          capabilities: [],
          module_id: "lenso/identity",
          optional: false,
          version_requirement: "*",
        },
      ],
    });
    expect(supportTicket.name).toBe("acme/support-ticket");
    expect(supportTicket.version).toBe("0.1.0");
    expect(supportTicket.dependencies).toEqual(["lenso/identity"]);
    expect(JSON.parse(JSON.stringify(supportTicket))).not.toHaveProperty(
      "dependencies"
    );
    expect(JSON.parse(JSON.stringify(supportTicket))).not.toHaveProperty("name");
    expect(JSON.parse(JSON.stringify(supportTicket))).not.toHaveProperty(
      "version"
    );

    expect(
      defineService({
        compatibility: {
          required_host_features: ["service.status"],
        },
        deployment: {
          commands: ["pnpm start"],
          target: "container-paas",
        },
        install: {
          services: [
            {
              command: "pnpm start",
              name: "support-service",
              readyUrl: "http://127.0.0.1:4110/lenso/service/v1/status",
            },
          ],
        },
        modules: [supportTicket],
        name: "support-service",
        requiredEnv: ["PORT"],
      })
    ).toEqual({
      compatibility: {
        required_host_features: ["service.status"],
      },
      deployment: {
        commands: ["pnpm start"],
        target: "container-paas",
      },
      install: {
        services: [
          {
            command: "pnpm start",
            name: "support-service",
            readyUrl: "http://127.0.0.1:4110/lenso/service/v1/status",
          },
        ],
      },
      modules: [supportTicket],
      name: "support-service",
      protocol: "lenso.service.v1",
      required_env: ["PORT"],
      status_path: "/lenso/service/v1/status",
      transports: ["http"],
      version: "0.1.0",
    });
  });

  test("defines HTTP route declarations", () => {
    expect(
      defineProviderModule({
        httpRoutes: [
          getRoute("/contacts/{id}", {
            capability: "crm.contacts.read",
            displayName: "Fetch Contact",
            storyTitle: "Fetch Contact",
          }),
        ],
        name: "crm",
      })
    ).toMatchObject({
      http_routes: [
        {
          capability: "crm.contacts.read",
          display_name: "Fetch Contact",
          method: "GET",
          path: "/contacts/{id}",
          story_title: "Fetch Contact",
        },
      ],
    });
  });

  test("preserves service operation metadata in module manifests", () => {
    const operation = {
      idempotency: "requires_key" as const,
      inputSchema: { type: "object" },
      operationId: "crm.contacts.sync",
      outputSchema: { type: "object" },
      safeProbe: {
        expectStatus: 204,
        method: "POST",
        path: "/contacts/sync/probe",
      },
      summary: "Sync contacts",
      timeoutMs: 5000,
    };

    expect(
      defineModule({
        admin: declarativeCustom({
          actions: [
            adminAction("sync_contacts", {
              capability: "crm.contacts.sync",
              label: "Sync contacts",
              operation,
            }),
          ],
        }),
        eventHandlers: [
          eventHandler(
            "sync_contact_on_user_registered",
            "identity.user_registered.v1",
            { operation }
          ),
        ],
        httpRoutes: [
          postRoute("/contacts/sync", {
            capability: "crm.contacts.sync",
            operation,
          }),
        ],
        name: "crm",
        runtimeFunctions: [
          runtimeFunction("crm.contacts.sync.v1", { operation }),
        ],
      })
    ).toMatchObject({
      admin: {
        actions: [{ operation }],
      },
      events: {
        handlers: [{ operation }],
      },
      http_routes: [{ operation }],
      runtime: {
        functions: [{ operation }],
      },
    });
  });

  test("defines runtime function declarations", () => {
    expect(
      defineProviderModule({
        name: "crm",
        runtimeFunctions: [
          runtimeFunction("crm.contacts.enrich.v1", {
            inputSchema: "crm.contacts.enrich.v1",
            queue: "crm",
            retryPolicy: {
              initial_delay_ms: 1000,
              max_attempts: 3,
            },
            version: 1,
          }),
        ],
      })
    ).toMatchObject({
      runtime: {
        functions: [
          {
            input_schema: "crm.contacts.enrich.v1",
            name: "crm.contacts.enrich.v1",
            queue: "crm",
            retry_policy: {
              initial_delay_ms: 1000,
              max_attempts: 3,
            },
            version: 1,
          },
        ],
      },
    });
  });

  test("defines lifecycle activation declarations", () => {
    expect(
      defineProviderModule({
        lifecycle: lifecycle({
          activationJobs: [
            everyStartup("sync contacts on startup", "crm.contacts.enrich.v1", {
              input: { reason: "worker_startup" },
            }),
          ],
          startupChecks: [
            {
              function_name: "crm.contacts.enrich.v1",
              kind: "function_registered",
              name: "contacts enrich function is registered",
              required: true,
            },
          ],
        }),
        name: "crm",
      })
    ).toMatchObject({
      lifecycle: {
        activation_jobs: [
          {
            function_name: "crm.contacts.enrich.v1",
            input: { reason: "worker_startup" },
            name: "sync contacts on startup",
            required: true,
            run_policy: "every_startup",
          },
        ],
        startup_checks: [
          {
            function_name: "crm.contacts.enrich.v1",
            kind: "function_registered",
            name: "contacts enrich function is registered",
            required: true,
          },
        ],
      },
    });
  });

  test("serves the manifest through the service module protocol", async () => {
    const manifest = defineProviderModule({ name: "billing" });
    const served = await serveModuleProvider(manifest, { port: 0 });
    try {
      await expect(
        fetch(served.manifestUrl).then((response) => response.json())
      ).resolves.toMatchObject({
        name: "billing",
        source: "service",
      });
      await expect(
        fetch(served.statusUrl).then((response) => response.json())
      ).resolves.toMatchObject({
        moduleName: "billing",
        serviceName: "api",
        state: "ready",
      });
      const missing = await fetch(`${served.baseUrl}/missing`, {
        headers: {
          "x-lenso-correlation-id": "corr-service-kit",
          "x-request-id": "req-service-kit",
        },
      });
      expect(missing.headers.get("content-type")).toBe(
        "application/problem+json"
      );
      await expect(missing.json()).resolves.toEqual({
        code: "not_found",
        correlation_id: "corr-service-kit",
        detail: "billing service module endpoint not found",
        errors: [],
        request_id: "req-service-kit",
        status: 404,
        title: "Not found",
        type: "https://lenso.dev/problems/not_found",
      });
    } finally {
      await served.close();
    }
  });

  test("serves a service manifest, status, and module HTTP handlers", async () => {
    const service = defineService({
      modules: [
        defineModule({
          capabilities: ["support_ticket.tickets.read"],
          httpRoutes: [
            getRoute("/tickets/{id}", {
              capability: "support_ticket.tickets.read",
              displayName: "Get Ticket",
              storyTitle: "Get Ticket",
            }),
          ],
          name: "acme/support-ticket",
        }),
      ],
      name: "support-service",
    });
    const served = await serveService(service, {
      modules: {
        "acme/support-ticket": {
          http: {
            "GET /tickets/{id}": ({ params }) => ({
              ticket: { id: params.id },
            }),
          },
        },
      },
      port: 0,
    });

    try {
      await expect(
        fetch(served.manifestUrl).then((response) => response.json())
      ).resolves.toMatchObject({
        modules: [
          {
            module_id: "acme/support-ticket",
            protocol: "lenso.module-manifest.v1",
          },
        ],
        name: "support-service",
        protocol: "lenso.service.v1",
      });
      await expect(
        fetch(served.statusUrl).then((response) => response.json())
      ).resolves.toMatchObject({
        modules: [{ name: "acme/support-ticket", version: "0.1.0" }],
        serviceName: "support-service",
        state: "ready",
      });
      await expect(
        fetch(`${served.baseUrl}/modules/acme/support-ticket/tickets/ticket_1`).then(
          (response) => response.json()
        )
      ).resolves.toEqual({
        ticket: { id: "ticket_1" },
      });
    } finally {
      await served.close();
    }
  });

  test("serves strict Provider Core identity only to the exact local bearer", async () => {
    const service = defineService({
      modules: [defineModule({ name: "support-ticket" })],
      name: "support-service",
    });
    const bearerToken = "local-enrollment-token";
    const served = await serveService(service, {
      port: 0,
      providerCore: {
        bearerToken,
        serviceId: "support-service",
        servicePrincipal: "service:support-service",
        serviceRevision: "release:sha256:0123456789abcdef",
      },
    });

    try {
      expect(served.systemPlaneCoreUrl).toBe(
        `${new URL(served.baseUrl).origin}/system-plane/v1`
      );

      const missingCredential = await fetch(served.systemPlaneCoreUrl);
      const missingBody = await missingCredential.json();
      expect(missingCredential.status).toBe(401);
      expect(missingCredential.headers.get("content-type")).toBe(
        "application/problem+json"
      );
      expect(missingBody).toEqual({
        code: "system_plane_bearer_required",
        correlation_id: null,
        detail:
          "System Plane Core access requires the configured local bearer credential",
        errors: [],
        request_id: null,
        status: 401,
        title: "Unauthorized",
        type: "https://lenso.dev/problems/system_plane_bearer_required",
      });

      const wrongCredential = await fetch(served.systemPlaneCoreUrl, {
        headers: { authorization: "Bearer wrong-token" },
      });
      expect(wrongCredential.status).toBe(401);
      await expect(wrongCredential.json()).resolves.toEqual(missingBody);

      const accepted = await fetch(served.systemPlaneCoreUrl, {
        headers: { authorization: `Bearer ${bearerToken}` },
      });
      expect(accepted.status).toBe(200);
      const coreDocument = await accepted.json();
      expect(coreDocument).toEqual({
        protocol: "lenso.system-plane.v1",
        serviceId: "support-service",
        servicePrincipal: "service:support-service",
        serviceRevision: "release:sha256:0123456789abcdef",
      });
      expect(JSON.stringify([missingBody, coreDocument])).not.toContain(
        bearerToken
      );
      const wrongMethod = await fetch(served.systemPlaneCoreUrl, {
        headers: { authorization: `Bearer ${bearerToken}` },
        method: "POST",
      });
      expect(wrongMethod.status).toBe(404);
    } finally {
      await served.close();
    }
  });

  test("serves the locked Provider v1 descriptor, invocation, recovery, and acknowledgement", async () => {
    const lockedDigest = (character: string) =>
      `sha256:${character.repeat(64)}`;
    const manifest = {
      capabilities: ["taste.profiles.read"],
      module_id: "taste/profile",
      protocol: "lenso.module-manifest.v1",
    };
    const manifestDigest = `sha256:${createHash("sha256")
      .update(JSON.stringify(manifest))
      .digest("hex")}`;
    const moduleRelease = {
      compatibility: {},
      delivery: {
        contract_digests: [lockedDigest("4")],
        export: "taste-profile",
        kind: "service",
        responsibility_profile: "provider",
        service_id: "taste/service",
        service_release_digest: lockedDigest("5"),
        service_release_version: "0.1.0",
      },
      manifest,
      manifest_digest: manifestDigest,
      module_id: "taste/profile",
      protocol: "lenso.module-release.v1",
      version: "0.1.0",
    };
    const moduleReleaseDigest = `sha256:${createHash("sha256")
      .update(JSON.stringify(moduleRelease))
      .digest("hex")}`;
    let observedActor: unknown;
    const service = defineService({
      modules: [
        defineModule({
          httpRoutes: [getRoute("/profiles/{id}")],
          name: "taste-profile",
        }),
      ],
      name: "taste-service",
    });
    const served = await serveService(service, {
      modules: {
        "taste-profile": {
          http: {
            "GET /profiles/{id}": ({ actor, params }) => {
              observedActor = actor;
              return { profile: { id: params.id } };
            },
          },
        },
      },
      port: 0,
      providerV1: {
        exports: [
          {
            contractDigests: { http: lockedDigest("4") },
            exportKey: "taste-profile",
            manifest,
            manifestDigest,
            moduleId: "taste/profile",
            moduleReleaseDigest,
            moduleVersion: "0.1.0",
          },
        ],
        moduleReleases: { "taste-profile": moduleRelease },
        protocolContractDigest: lockedDigest("1"),
        runtimeInstanceId: "taste-local-1",
        serviceId: "taste/service",
        serviceReleaseDigest: lockedDigest("5"),
        serviceReleaseVersion: "0.1.0",
      },
    });

    try {
      const origin = new URL(served.baseUrl).origin;
      const providerUrl = `${origin}/lenso/provider/v1`;
      const descriptor = await fetch(providerUrl).then((response) => response.json());
      expect(descriptor).toMatchObject({
        features: [],
        protocol: "lenso.provider.v1",
        serviceId: "taste/service",
        exports: [{ exportKey: "taste-profile", moduleId: "taste/profile" }],
      });
      expect(descriptor).not.toHaveProperty("moduleReleases");
      await expect(
        fetch(`${providerUrl}/exports/taste-profile/module-release`).then(
          (response) => response.json()
        )
      ).resolves.toEqual(moduleRelease);
      const invocation = {
        actor: {
          kind: "user",
          scopes: ["taste.profiles.read"],
          user_id: "taste-user",
        },
        attempt: 1,
        contentType: "application/json",
        correlationId: "correlation-1",
        deadline: "2026-08-13T01:00:00Z",
        protocol: "lenso.provider.v1",
        invocationId: "invocation-1",
        serviceReleaseDigest: lockedDigest("5"),
        exportKey: "taste-profile",
        moduleReleaseDigest,
        manifestDigest,
        operationKind: "http_route",
        operationName: "GET /profiles/{id}",
        operationVersion: "1",
        mode: "durable",
        inputContractDigest: lockedDigest("4"),
        outputContractDigest: lockedDigest("4"),
        payload: {
          method: "GET",
          declared_path: "/profiles/{id}",
          path_params: { id: "profile-1" },
        },
        requestId: "request-1",
        trace: { baggage: [], span_id: null, trace_id: null },
      };
      const invoked = await fetch(`${providerUrl}/exports/taste-profile/http:invoke`, {
        body: JSON.stringify(invocation),
        headers: { "content-type": "application/json" },
        method: "POST",
      });
      expect(invoked.status).toBe(200);
      const outcome = (await invoked.json()) as { outcomeDigest: string };
      expect(outcome).toMatchObject({
        invocationId: "invocation-1",
        result: { body: { profile: { id: "profile-1" } }, status_code: 200 },
        status: "succeeded",
      });
      expect(observedActor).toEqual(invocation.actor);
      await expect(
        fetch(`${providerUrl}/invocations/invocation-1`).then((response) => response.json())
      ).resolves.toEqual(outcome);
      const acknowledged = await fetch(`${providerUrl}/invocations/invocation-1:ack`, {
        body: JSON.stringify({
          invocationId: "invocation-1",
          outcomeDigest: outcome.outcomeDigest,
        }),
        headers: { "content-type": "application/json" },
        method: "POST",
      });
      expect(acknowledged.status).toBe(200);
    } finally {
      await served.close();
    }
  });

  test("does not advertise durable invocations for the process-local default Store", async () => {
    const fixture = providerTestFixture();
    const served = await serveService(fixture.service, {
      port: 0,
      providerV1: fixture.providerV1,
    });

    try {
      const providerUrl = `${new URL(served.baseUrl).origin}/lenso/provider/v1`;
      await expect(
        fetch(providerUrl).then((response) => response.json())
      ).resolves.toMatchObject({ features: [] });
    } finally {
      await served.close();
    }

    await expect(
      serveService(fixture.service, {
        port: 0,
        providerV1: {
          ...fixture.providerV1,
          features: ["durable_invocations"],
        },
      })
    ).rejects.toThrow(
      "providerV1 durable_invocations requires a durable invocationStore"
    );
  });

  test("uses RFC 8785 object-key order and rejects non-JSON digest inputs", async () => {
    const fixture = providerTestFixture();
    const manifest = { "2": "two", "10": "ten" };
    const manifestDigest = `sha256:${createHash("sha256")
      .update('{"10":"ten","2":"two"}')
      .digest("hex")}`;
    const providerV1: ProviderV1Options = {
      ...fixture.providerV1,
      exports: [
        {
          ...fixture.providerV1.exports[0]!,
          manifest,
          manifestDigest,
        },
      ],
      moduleReleases: undefined,
    };
    const served = await serveService(fixture.service, {
      port: 0,
      providerV1,
    });
    await served.close();

    await expect(
      serveService(fixture.service, {
        port: 0,
        providerV1: {
          ...providerV1,
          exports: [
            {
              ...providerV1.exports[0]!,
              manifest: { invalid: "\ud800" },
            },
          ],
        },
      })
    ).rejects.toThrow("Canonical JSON cannot contain an unpaired surrogate");

    await expect(
      serveService(fixture.service, {
        port: 0,
        providerV1: {
          ...providerV1,
          exports: [
            {
              ...providerV1.exports[0]!,
              manifest: { invalid: new Array(1) },
            },
          ],
        },
      })
    ).rejects.toThrow("Canonical JSON cannot contain a sparse array");
  });

  test("recovers one durable outcome after restart without executing the side effect again", async () => {
    const fixture = providerTestFixture();
    const backingStore = createMemoryProviderInvocationStore();
    const adapters: ProviderInvocationStore[] = [];
    let executions = 0;
    const start = () => {
      const invocationStore = durableProviderTestStore(backingStore);
      adapters.push(invocationStore);
      return serveService(fixture.service, {
        modules: {
          "taste-profile": {
            http: {
              "GET /profiles/{id}": ({ params }) => {
                executions += 1;
                return providerSucceeded(
                  { profile: { id: params.id } },
                  {
                    effectEvidence: [
                      { kind: "remote_receipt", receiptId: "receipt-1" },
                    ],
                  }
                );
              },
            },
          },
        },
        port: 0,
        providerV1: { ...fixture.providerV1, invocationStore },
      });
    };

    const first = await start();
    let outcomeDigest = "";
    try {
      const providerUrl = `${new URL(first.baseUrl).origin}/lenso/provider/v1`;
      const descriptor = await fetch(providerUrl).then((response) =>
        response.json()
      );
      expect(descriptor.features).toContain("durable_invocations");
      const discardedResponse = await fetch(
        `${providerUrl}/exports/taste-profile/http:invoke`,
        {
          body: JSON.stringify(fixture.invocation),
          headers: { "content-type": "application/json" },
          method: "POST",
        }
      );
      expect(discardedResponse.status).toBe(200);
      await discardedResponse.body?.cancel();
      expect(executions).toBe(1);
    } finally {
      await first.close();
    }

    const restarted = await start();
    try {
      expect(adapters).toHaveLength(2);
      expect(adapters[0]).not.toBe(adapters[1]);
      const providerUrl = `${new URL(restarted.baseUrl).origin}/lenso/provider/v1`;
      const replayedResponse = await fetch(
        `${providerUrl}/exports/taste-profile/http:invoke`,
        {
          body: JSON.stringify(fixture.invocation),
          headers: { "content-type": "application/json" },
          method: "POST",
        }
      );
      expect(replayedResponse.status).toBe(200);
      const replayed = (await replayedResponse.json()) as {
        outcomeDigest: string;
      };
      outcomeDigest = replayed.outcomeDigest;
      expect(replayed).toMatchObject({
        effectEvidence: [
          { kind: "remote_receipt", receiptId: "receipt-1" },
        ],
        result: {
          body: { profile: { id: "profile-1" } },
          status_code: 200,
        },
        status: "succeeded",
      });
      expect(executions).toBe(1);
      await expect(
        fetch(`${providerUrl}/invocations/invocation-1`).then((response) =>
          response.json()
        )
      ).resolves.toEqual(replayed);

      for (let attempt = 0; attempt < 2; attempt += 1) {
        const acknowledged = await fetch(
          `${providerUrl}/invocations/invocation-1:ack`,
          {
            body: JSON.stringify({
              invocationId: "invocation-1",
              outcomeDigest,
            }),
            headers: { "content-type": "application/json" },
            method: "POST",
          }
        );
        expect(acknowledged.status).toBe(200);
      }
      const stored = await durableProviderTestStore(backingStore).get(
        "invocation-1"
      );
      expect(stored).toMatchObject({
        acknowledgedOutcomeDigest: outcomeDigest,
        phase: "completed",
      });
      expect(stored?.acknowledgedAt).not.toBeNull();
    } finally {
      await restarted.close();
    }
  });

  test("replays canonical requests and rejects rebinding one invocation id", async () => {
    const fixture = providerTestFixture();
    const invocationStore = durableProviderTestStore();
    let executions = 0;
    const served = await serveService(fixture.service, {
      modules: {
        "taste-profile": {
          http: {
            "GET /profiles/{id}": ({ params }) => {
              executions += 1;
              return { profile: { id: params.id } };
            },
          },
        },
      },
      port: 0,
      providerV1: { ...fixture.providerV1, invocationStore },
    });

    try {
      const invokeUrl = `${new URL(served.baseUrl).origin}/lenso/provider/v1/exports/taste-profile/http:invoke`;
      const invoke = (body: unknown) =>
        fetch(invokeUrl, {
          body: JSON.stringify(body),
          headers: { "content-type": "application/json" },
          method: "POST",
        });
      const initial = await invoke(fixture.invocation);
      expect(initial.status).toBe(200);
      const initialOutcome = await initial.json();

      const reordered = {
        requestId: fixture.invocation.requestId,
        protocol: fixture.invocation.protocol,
        payload: {
          path_params: { id: "profile-1" },
          method: "GET",
          declared_path: "/profiles/{id}",
        },
        outputContractDigest: fixture.invocation.outputContractDigest,
        operationName: fixture.invocation.operationName,
        operationKind: fixture.invocation.operationKind,
        moduleReleaseDigest: fixture.invocation.moduleReleaseDigest,
        manifestDigest: fixture.invocation.manifestDigest,
        invocationId: fixture.invocation.invocationId,
        inputContractDigest: fixture.invocation.inputContractDigest,
        exportKey: fixture.invocation.exportKey,
        deadline: fixture.invocation.deadline,
        correlationId: fixture.invocation.correlationId,
        contentType: fixture.invocation.contentType,
        attempt: fixture.invocation.attempt,
        actor: fixture.invocation.actor,
        trace: fixture.invocation.trace,
        serviceReleaseDigest: fixture.invocation.serviceReleaseDigest,
        operationVersion: fixture.invocation.operationVersion,
        mode: fixture.invocation.mode,
      };
      const canonicalReplay = await invoke(reordered);
      expect(canonicalReplay.status).toBe(200);
      await expect(canonicalReplay.json()).resolves.toEqual(initialOutcome);
      expect(executions).toBe(1);

      const conflict = await invoke({ ...fixture.invocation, attempt: 2 });
      expect(conflict.status).toBe(409);
      await expect(conflict.json()).resolves.toMatchObject({
        error: {
          code: "invocation_identity_conflict",
          retryable: false,
        },
      });
      expect(executions).toBe(1);
    } finally {
      await served.close();
    }
  });

  test("returns typed pending, rejected, failed, and succeeded Provider outcomes", async () => {
    const fixture = providerTestFixture();
    const invocationStore = durableProviderTestStore();
    const served = await serveService(fixture.service, {
      modules: {
        "taste-profile": {
          http: {
            "GET /profiles/{id}": ({ params }) => {
              switch (params.id) {
                case "pending":
                  return providerPending({
                    error: {
                      code: "provider_pending",
                      message: "The remote receipt is not final",
                      providerTraceReference: "smtp-attempt-1",
                      retryAfterMs: 250,
                      retryable: true,
                    },
                  });
                case "rejected":
                  return providerRejected({
                    code: "recipient_rejected",
                    details: [
                      { field: "recipient", reason: "mailbox unavailable" },
                    ],
                    message: "The recipient was rejected",
                  });
                case "failed":
                  return providerFailed({
                    code: "smtp_unavailable",
                    message: "SMTP is temporarily unavailable",
                    providerTraceReference: "smtp-attempt-3",
                    retryAfterMs: 2_500,
                    retryable: true,
                  });
                default:
                  return providerSucceeded(
                    { profile: { id: params.id } },
                    {
                      effectEvidence: [
                        { kind: "smtp_receipt", receiptId: "receipt-1" },
                      ],
                      hostEffects: {
                        events: [
                          {
                            aggregateId: params.id,
                            aggregateType: "profile",
                            correlationId: "correlation-1",
                            eventId: "event-1",
                            eventName: "taste.profile-delivered.v1",
                            eventVersion: 1,
                            occurredAt: "2026-08-13T00:00:00.000Z",
                            payload: { profileId: params.id },
                            sourceModule: "taste/profile",
                          },
                        ],
                        runtimeFunctionRequests: [
                          {
                            actor: fixture.invocation.actor,
                            correlationId: "correlation-1",
                            functionName: "taste.confirm-receipt.v1",
                            input: { profileId: params.id },
                            maxAttempts: 3,
                            requestId: "function-request-1",
                          },
                        ],
                      },
                    }
                  );
              }
            },
          },
        },
      },
      port: 0,
      providerV1: { ...fixture.providerV1, invocationStore },
    });

    try {
      const invokeUrl = `${new URL(served.baseUrl).origin}/lenso/provider/v1/exports/taste-profile/http:invoke`;
      const invoke = async (status: string) => {
        const response = await fetch(invokeUrl, {
          body: JSON.stringify({
            ...fixture.invocation,
            invocationId: `invocation-${status}`,
            payload: {
              ...fixture.invocation.payload,
              path_params: { id: status },
            },
            requestId: `request-${status}`,
          }),
          headers: { "content-type": "application/json" },
          method: "POST",
        });
        return { body: await response.json(), status: response.status };
      };

      const pending = await invoke("pending");
      expect(pending.status).toBe(202);
      expect(pending.body).toMatchObject({
        error: {
          code: "provider_pending",
          providerTraceReference: "smtp-attempt-1",
          retryAfterMs: 250,
          retryable: true,
        },
        hostEffects: { events: [], runtimeFunctionRequests: [] },
        result: null,
        status: "pending",
      });

      const rejected = await invoke("rejected");
      expect(rejected.status).toBe(200);
      expect(rejected.body).toMatchObject({
        error: {
          code: "recipient_rejected",
          retryAfterMs: null,
          retryable: false,
        },
        status: "rejected",
      });

      const failed = await invoke("failed");
      expect(failed.status).toBe(200);
      expect(failed.body).toMatchObject({
        error: {
          code: "smtp_unavailable",
          providerTraceReference: "smtp-attempt-3",
          retryAfterMs: 2_500,
          retryable: true,
        },
        outcomeDigest:
          "sha256:a7d26349366917a4012bf47b4f207416171819f558dc69ca851c05146936681f",
        status: "failed",
      });

      const succeeded = await invoke("succeeded");
      expect(succeeded.status).toBe(200);
      expect(succeeded.body).toMatchObject({
        effectEvidence: [
          { kind: "smtp_receipt", receiptId: "receipt-1" },
        ],
        error: null,
        hostEffects: {
          events: [{ eventId: "event-1" }],
          runtimeFunctionRequests: [
            {
              functionName: "taste.confirm-receipt.v1",
              maxAttempts: 3,
              trace: { baggage: [], span_id: null, trace_id: null },
            },
          ],
        },
        status: "succeeded",
      });
    } finally {
      await served.close();
    }
  });

  test("rejects unbounded Provider retry metadata before making it durable", async () => {
    const fixture = providerTestFixture();
    const served = await serveService(fixture.service, {
      modules: {
        "taste-profile": {
          http: {
            "GET /profiles/{id}": ({ params }) =>
              providerFailed({
                code: "upstream_unavailable",
                message: "The upstream is unavailable",
                providerTraceReference:
                  params.id === "trace"
                    ? `trace\n${"x".repeat(1_000)}`
                    : "bounded-trace",
                retryAfterMs:
                  params.id === "delay"
                    ? providerV1OutcomeLimits.maxRetryAfterMs + 1
                    : 1_000,
                retryable: true,
              }),
          },
        },
      },
      port: 0,
      providerV1: {
        ...fixture.providerV1,
        invocationStore: durableProviderTestStore(),
      },
    });

    try {
      const invokeUrl = `${new URL(served.baseUrl).origin}/lenso/provider/v1/exports/taste-profile/http:invoke`;
      for (const invalid of ["trace", "delay"]) {
        const response = await fetch(invokeUrl, {
          body: JSON.stringify({
            ...fixture.invocation,
            invocationId: `invocation-unbounded-${invalid}`,
            payload: {
              ...fixture.invocation.payload,
              path_params: { id: invalid },
            },
          }),
          headers: { "content-type": "application/json" },
          method: "POST",
        });
        expect(response.status).toBe(200);
        await expect(response.json()).resolves.toMatchObject({
          error: { code: "provider_handler_failed", retryable: false },
          status: "failed",
        });
      }
    } finally {
      await served.close();
    }
  });

  test("fails closed before persisting unbounded or non-JSON outcomes", async () => {
    const fixture = providerTestFixture();
    const served = await serveService(fixture.service, {
      modules: {
        "taste-profile": {
          http: {
            "GET /profiles/{id}": ({ params }) =>
              params.id === "unbounded"
                ? providerSucceeded(
                    {},
                    {
                      effectEvidence: Array.from(
                        {
                          length:
                            providerV1OutcomeLimits.maxEffectEvidenceItems + 1,
                        },
                        (_, index) => ({ index })
                      ),
                    }
                  )
                : params.id === "sparse"
                  ? providerSucceeded({ values: new Array(1) })
                  : providerSucceeded({ value: Number.NaN }),
          },
        },
      },
      port: 0,
      providerV1: {
        ...fixture.providerV1,
        invocationStore: durableProviderTestStore(),
      },
    });

    try {
      const invokeUrl = `${new URL(served.baseUrl).origin}/lenso/provider/v1/exports/taste-profile/http:invoke`;
      for (const invalidResult of ["unbounded", "sparse", "non-json"]) {
        const response = await fetch(invokeUrl, {
          body: JSON.stringify({
            ...fixture.invocation,
            invocationId: `invocation-${invalidResult}`,
            payload: {
              ...fixture.invocation.payload,
              path_params: { id: invalidResult },
            },
          }),
          headers: { "content-type": "application/json" },
          method: "POST",
        });
        expect(response.status).toBe(200);
        await expect(response.json()).resolves.toMatchObject({
          error: {
            code: "provider_handler_failed",
            retryable: false,
          },
          hostEffects: { events: [], runtimeFunctionRequests: [] },
          result: null,
          status: "failed",
        });
      }
    } finally {
      await served.close();
    }
  });

  test("keeps Provider Core disabled unless exact local identity is configured", async () => {
    const service = defineService({
      modules: [defineModule({ name: "support-ticket" })],
      name: "support-service",
    });
    const served = await serveService(service, { port: 0 });

    try {
      expect(served.systemPlaneCoreUrl).toBeUndefined();
      const response = await fetch(
        `${new URL(served.baseUrl).origin}/system-plane/v1`,
        { headers: { authorization: "Bearer unused" } }
      );
      expect(response.status).toBe(404);
    } finally {
      await served.close();
    }
  });

  test("requires and verifies a Provider bearer outside loopback", async () => {
    const fixture = providerTestFixture();
    await expect(
      serveService(fixture.service, {
        host: "0.0.0.0",
        port: 0,
        providerV1: fixture.providerV1,
      })
    ).rejects.toThrow(
      "Provider V1 requires providerV1.bearerToken outside loopback"
    );

    const served = await serveService(fixture.service, {
      host: "0.0.0.0",
      port: 0,
      providerV1: {
        ...fixture.providerV1,
        bearerToken: "provider-network-secret",
      },
    });
    const descriptorUrl = `${new URL(served.baseUrl).origin.replace(
      "0.0.0.0",
      "127.0.0.1"
    )}/lenso/provider/v1`;
    try {
      expect((await fetch(descriptorUrl)).status).toBe(401);
      expect(
        (
          await fetch(descriptorUrl, {
            headers: { authorization: "Bearer wrong" },
          })
        ).status
      ).toBe(401);
      expect(
        (
          await fetch(descriptorUrl, {
            headers: { authorization: "Bearer provider-network-secret" },
          })
        ).status
      ).toBe(200);
    } finally {
      await served.close();
    }
  });

  test("rejects incomplete or non-loopback Provider Core configuration", async () => {
    const service = defineService({
      modules: [defineModule({ name: "support-ticket" })],
      name: "support-service",
    });
    const validCore = {
      bearerToken: "local-enrollment-token",
      serviceId: "support-service",
      servicePrincipal: "service:support-service",
      serviceRevision: "release:sha256:0123456789abcdef",
    };

    for (const field of [
      "bearerToken",
      "serviceId",
      "servicePrincipal",
      "serviceRevision",
    ] as const) {
      await expect(
        serveService(service, {
          port: 0,
          providerCore: { ...validCore, [field]: " \t" },
        })
      ).rejects.toThrow(`providerCore.${field} must be a non-empty string`);
    }

    await expect(
      serveService(service, {
        host: "0.0.0.0",
        port: 0,
        providerCore: validCore,
      })
    ).rejects.toThrow("providerCore requires a loopback host");
  });

  test("defines schema-admin entities and serves list/detail data", async () => {
    const contacts = defineSchemaEntity({
      fields: [
        textField("email"),
        textField("name", { label: "Full name" }),
        integerField("score", { nullable: true }),
        booleanField("active"),
        timestampField("created_at"),
        jsonField("metadata"),
      ],
      label: "Contacts",
      name: "contacts",
      readCapability: "crm.contacts.read",
    });
    const manifest = defineProviderModule({
      admin: schemaAdmin([contacts]),
      capabilities: ["crm.contacts.read"],
      name: "crm",
    });
    expect(manifest.admin).toMatchObject({
      entities: [
        {
          fields: [
            {
              field_type: { kind: "string" },
              label: "Email",
              name: "email",
              nullable: false,
            },
            {
              field_type: { kind: "string" },
              label: "Full name",
              name: "name",
            },
            {
              field_type: { kind: "integer" },
              name: "score",
              nullable: true,
            },
            { field_type: { kind: "boolean" }, name: "active" },
            { field_type: { kind: "timestamp" }, name: "created_at" },
            { field_type: { kind: "json" }, name: "metadata" },
          ],
          name: "contacts",
          read_capability: "crm.contacts.read",
        },
      ],
      kind: "schema",
    });

    const served = await serveModuleProvider(manifest, {
      data: {
        contacts: {
          detail: (id) =>
            id === "contact_1" ? { email: "ada@example.com", id } : null,
          list: ({ limit }) => ({
            next_cursor: null,
            records: [{ email: "ada@example.com", limit }],
          }),
        },
      },
      port: 0,
    });
    try {
      await expect(
        fetch(
          `${served.baseUrl}/http/admin/contacts?limit=2`
        ).then((response) => response.json())
      ).resolves.toEqual({
        next_cursor: null,
        records: [{ email: "ada@example.com", limit: 2 }],
      });
      await expect(
        fetch(
          `${served.baseUrl}/http/admin/contacts/contact_1`
        ).then((response) => response.json())
      ).resolves.toEqual({
        record: { email: "ada@example.com", id: "contact_1" },
      });
    } finally {
      await served.close();
    }
  });

  test("defines declarative admin actions", () => {
    const contacts = defineSchemaEntity({
      fields: [textField("email")],
      label: "Contacts",
      name: "contacts",
      readCapability: "crm.contacts.read",
    });
    const manifest = defineProviderModule({
      admin: declarativeCustom({
        actions: [
          adminAction("sync_contacts", {
            capability: "crm.contacts.sync",
            confirmation: actionConfirmation("Sync remote contacts now?", {
              requiredPhrase: "SYNC",
            }),
            dangerLevel: "medium",
            inputFields: [
              actionBooleanField("dry_run", {
                description: "Preview the sync without writing remote data",
                label: "Dry run",
              }),
            ],
            label: "Sync contacts",
          }),
        ],
        fallbackSchema: adminSchema([contacts]),
        pages: [
          declarativePage("dashboard", {
            sections: [
              declarativeSection("contacts", {
                component: entityTable("contacts"),
              }),
              declarativeSection("metrics", {
                component: metricStrip([
                  metricBinding("Pending contacts", "$.pending_contacts"),
                ]),
              }),
              declarativeSection("health", {
                component: queryValue("health", {
                  capability: "crm.health.read",
                  valuePath: "metrics.contacts",
                }),
              }),
            ],
          }),
        ],
      }),
      capabilities: [
        "crm.contacts.read",
        "crm.contacts.sync",
        "crm.health.read",
      ],
      name: "crm",
    });

    expect(manifest.admin).toEqual({
      actions: [
        {
          capability: "crm.contacts.sync",
          confirmation: {
            message: "Sync remote contacts now?",
            required_phrase: "SYNC",
          },
          danger_level: "medium",
          input_schema: {
            fields: [
              {
                description: "Preview the sync without writing remote data",
                field_type: { kind: "boolean" },
                label: "Dry run",
                name: "dry_run",
                required: false,
              },
            ],
          },
          label: "Sync contacts",
          name: "sync_contacts",
        },
      ],
      fallback_schema: {
        entities: [
          {
            fields: [
              {
                field_type: { kind: "string" },
                label: "Email",
                name: "email",
                nullable: false,
              },
            ],
            label: "Contacts",
            name: "contacts",
            read_capability: "crm.contacts.read",
          },
        ],
      },
      kind: "declarative_custom",
      pages: [
        {
          label: "Dashboard",
          name: "dashboard",
          sections: [
            {
              component: {
                entity: "contacts",
                kind: "entity_table",
              },
              label: "Contacts",
              name: "contacts",
            },
            {
              component: {
                kind: "metric_strip",
                metrics: [
                  {
                    label: "Pending contacts",
                    value_path: "$.pending_contacts",
                  },
                ],
              },
              label: "Metrics",
              name: "metrics",
            },
            {
              component: {
                capability: "crm.health.read",
                kind: "query_value",
                query: "health",
                value_path: "metrics.contacts",
              },
              label: "Health",
              name: "health",
            },
          ],
        },
      ],
    });
  });

  test("serves declared HTTP routes with params and request body", async () => {
    const manifest = defineProviderModule({
      httpRoutes: [
        getRoute("/contacts/{id}", { capability: "crm.contacts.read" }),
        postRoute("/contacts", { capability: "crm.contacts.write" }),
      ],
      name: "crm",
    });
    const served = await serveModuleProvider(manifest, {
      http: {
        "GET /contacts/{id}": ({ params }) => ({
          email: "ada@example.com",
          id: params.id,
        }),
        "POST /contacts": ({ body }) => ({
          body: { contact: body },
          statusCode: 201,
        }),
      },
      port: 0,
    });
    try {
      await expect(
        fetch(`${served.baseUrl}/contacts/contact_1`).then((response) =>
          response.json()
        )
      ).resolves.toEqual({
        email: "ada@example.com",
        id: "contact_1",
      });
      const createResponse = await fetch(`${served.baseUrl}/contacts`, {
        body: JSON.stringify({ email: "grace@example.com" }),
        headers: { "content-type": "application/json" },
        method: "POST",
      });
      expect(createResponse.status).toBe(201);
      await expect(createResponse.json()).resolves.toEqual({
        contact: { email: "grace@example.com" },
      });
    } finally {
      await served.close();
    }
  });

  test("serves admin action invocations", async () => {
    const manifest = defineProviderModule({
      admin: declarativeCustom({
        actions: [
          adminAction("sync_contacts", {
            capability: "crm.contacts.sync",
            label: "Sync contacts",
          }),
        ],
      }),
      name: "crm",
    });
    const served = await serveModuleProvider(manifest, {
      actions: {
        sync_contacts: ({ action, input }) => ({
          action,
          dry_run:
            typeof input === "object" && input !== null && "dry_run" in input
              ? input.dry_run
              : false,
          synced: true,
        }),
      },
      port: 0,
    });
    try {
      const invokeResponse = await fetch(
        `${served.baseUrl}/http/admin/actions/sync_contacts`,
        {
          body: JSON.stringify({ dry_run: true }),
          headers: { "content-type": "application/json" },
          method: "POST",
        }
      );
      expect(invokeResponse.status).toBe(200);
      await expect(invokeResponse.json()).resolves.toEqual({
        result: {
          action: "sync_contacts",
          dry_run: true,
          synced: true,
        },
      });

      const missingResponse = await fetch(
        `${served.baseUrl}/http/admin/actions/missing`,
        {
          body: JSON.stringify({}),
          headers: { "content-type": "application/json" },
          method: "POST",
        }
      );
      expect(missingResponse.status).toBe(404);
      expect(missingResponse.headers.get("content-type")).toBe(
        "application/problem+json"
      );
      await expect(missingResponse.json()).resolves.toMatchObject({
        code: "not_found",
        detail: "missing admin action handler not found",
        status: 404,
        type: "https://lenso.dev/problems/not_found",
      });
    } finally {
      await served.close();
    }
  });

  test("serves admin query values", async () => {
    const manifest = defineProviderModule({
      admin: declarativeCustom({
        pages: [
          declarativePage("dashboard", {
            sections: [
              declarativeSection("health", {
                component: queryValue("health", {
                  capability: "crm.health.read",
                  valuePath: "metrics.contacts",
                }),
              }),
            ],
          }),
        ],
      }),
      name: "crm",
    });
    const served = await serveModuleProvider(manifest, {
      port: 0,
      queries: {
        health: ({ query }) => ({ metrics: { contacts: 2 }, query }),
      },
    });
    try {
      const queryResponse = await fetch(
        `${served.baseUrl}/http/admin/queries/health`
      );
      expect(queryResponse.status).toBe(200);
      await expect(queryResponse.json()).resolves.toEqual({
        data: {
          metrics: { contacts: 2 },
          query: "health",
        },
      });
    } finally {
      await served.close();
    }
  });

  test("serves runtime function invocations", async () => {
    const manifest = defineProviderModule({
      name: "crm",
      runtimeFunctions: [runtimeFunction("crm.contacts.enrich.v1")],
    });
    const served = await serveModuleProvider(manifest, {
      port: 0,
      runtime: {
        "crm.contacts.enrich.v1": ({ input, invocation }) => ({
          enriched: true,
          function_run_id: invocation.function_run_id,
          input,
        }),
      },
    });
    try {
      await expect(
        fetch(
          `${served.baseUrl}/runtime/functions/crm.contacts.enrich.v1/invoke`,
          {
            body: JSON.stringify({
              actor: { id: "worker", kind: "service", scopes: [] },
              attempt: 1,
              correlation_id: "corr_1",
              function_name: "crm.contacts.enrich.v1",
              function_run_id: "fnrun_1",
              input: { contact_id: "contact_1" },
              request_id: "req_1",
              trace: { span_id: "span_1", trace_id: "trace_1" },
            }),
            headers: { "content-type": "application/json" },
            method: "POST",
          }
        ).then((response) => response.json())
      ).resolves.toEqual({
        output: {
          enriched: true,
          function_run_id: "fnrun_1",
          input: { contact_id: "contact_1" },
        },
      });
    } finally {
      await served.close();
    }
  });

  test("serves event handler invocations", async () => {
    const manifest = defineProviderModule({
      eventHandlers: [
        eventHandler(
          "sync_contact_on_user_registered",
          "identity.user_registered.v1"
        ),
      ],
      name: "crm",
    });
    const served = await serveModuleProvider(manifest, {
      events: {
        sync_contact_on_user_registered: ({ event }) => ({
          actions: [
            {
              function_name: "crm.contacts.enrich.v1",
              input: { contact_id: event.aggregate_id },
              type: "enqueue_function",
            },
          ],
        }),
      },
      port: 0,
    });
    try {
      await expect(
        fetch(
          `${served.baseUrl}/events/handlers/sync_contact_on_user_registered/invoke`,
          {
            body: JSON.stringify({
              actor: { kind: "user", scopes: [], user_id: "usr_actor" },
              aggregate_id: "usr_1",
              aggregate_type: "user",
              correlation_id: "corr_1",
              event_name: "identity.user_registered.v1",
              event_version: 1,
              handler_name: "sync_contact_on_user_registered",
              headers: {},
              outbox_event_id: "evt_1",
              payload: { email: "ada@example.com" },
              request_id: "evt_1:sync_contact_on_user_registered",
              source_module: "identity",
              trace: { span_id: "span_1", trace_id: "trace_1" },
            }),
            headers: { "content-type": "application/json" },
            method: "POST",
          }
        ).then((response) => response.json())
      ).resolves.toEqual({
        actions: [
          {
            function_name: "crm.contacts.enrich.v1",
            input: { contact_id: "usr_1" },
            type: "enqueue_function",
          },
        ],
      });
    } finally {
      await served.close();
    }
  });

  test("reads Lenso invocation context headers from Node requests", () => {
    const request = {
      headers: {
        traceparent: "00-trace-span-01",
        "x-lenso-actor-kind": "service",
        "x-lenso-causation-id": "cause_1",
        "x-lenso-correlation-id": ["corr_1", "corr_ignored"],
        "x-lenso-module": "crm",
        "x-lenso-operation": "crm.contacts.sync",
        "x-lenso-operation-kind": "runtime_function",
        "x-lenso-provider": "acme",
        "x-request-id": "req_1",
      },
    } as unknown as IncomingMessage;

    expect(readLensoInvocationContext(request)).toEqual({
      actorKind: "service",
      causationId: "cause_1",
      correlationId: "corr_1",
      moduleName: "crm",
      operationId: "crm.contacts.sync",
      operationKind: "runtime_function",
      providerName: "acme",
      requestId: "req_1",
      traceparent: "00-trace-span-01",
    });
    expect(
      readLensoInvocationContext({ headers: {} } as unknown as IncomingMessage)
        .requestId
    ).toBeUndefined();
  });

  test("serves the service module gRPC JSON envelope protocol", async () => {
    const manifest = defineProviderModule({
      admin: declarativeCustom({
        pages: [
          declarativePage("dashboard", {
            sections: [
              declarativeSection("health", {
                component: queryValue("health", {
                  capability: "crm.health.read",
                  valuePath: "metrics.contacts",
                }),
              }),
            ],
          }),
        ],
      }),
      eventHandlers: [
        eventHandler(
          "sync_contact_on_user_registered",
          "identity.user_registered.v1"
        ),
      ],
      name: "crm",
      runtimeFunctions: [runtimeFunction("crm.contacts.enrich.v1")],
    });
    const served = await serveModuleProviderGrpc(manifest, {
      events: {
        sync_contact_on_user_registered: ({ event }) => ({
          actions: [
            {
              function_name: "crm.contacts.enrich.v1",
              input: { contact_id: event.aggregate_id },
              type: "enqueue_function",
            },
          ],
        }),
      },
      port: 0,
      queries: {
        health: ({ query }) => ({ metrics: { contacts: 2 }, query }),
      },
      runtime: {
        "crm.contacts.enrich.v1": ({ input }) => ({ input, synced: true }),
      },
    });
    const client = connect(served.baseUrl.replace("grpc://", "http://"));
    try {
      await expect(
        grpcUnary(client, "/lenso.service.module.v1.ServiceModule/GetManifest", {})
      ).resolves.toMatchObject({
        name: "crm",
        runtime: {
          functions: [{ name: "crm.contacts.enrich.v1" }],
        },
      });
      await expect(
        grpcUnary(client, "/lenso.service.module.v1.ServiceModule/InvokeFunction", {
          actor: { kind: "user", scopes: [] },
          attempt: 1,
          correlation_id: "corr_1",
          function_name: "crm.contacts.enrich.v1",
          function_run_id: "fnrun_1",
          input: { contact_id: "usr_1" },
          request_id: "req_1",
          trace: { span_id: "span_1", trace_id: "trace_1" },
        })
      ).resolves.toEqual({
        output: {
          input: { contact_id: "usr_1" },
          synced: true,
        },
      });
      await expect(
        grpcUnary(client, "/lenso.service.module.v1.ServiceModule/QueryAdminValue", {
          query: "health",
        })
      ).resolves.toEqual({
        data: {
          metrics: { contacts: 2 },
          query: "health",
        },
      });
      await expect(
        grpcUnary(client, "/lenso.service.module.v1.ServiceModule/HandleEvent", {
          actor: { kind: "user", scopes: [], user_id: "usr_actor" },
          aggregate_id: "usr_1",
          aggregate_type: "user",
          correlation_id: "corr_1",
          event_name: "identity.user_registered.v1",
          event_version: 1,
          handler_name: "sync_contact_on_user_registered",
          headers: {},
          outbox_event_id: "evt_1",
          payload: { email: "ada@example.com" },
          request_id: "evt_1:sync_contact_on_user_registered",
          source_module: "identity",
          trace: { span_id: "span_1", trace_id: "trace_1" },
        })
      ).resolves.toEqual({
        actions: [
          {
            function_name: "crm.contacts.enrich.v1",
            input: { contact_id: "usr_1" },
            type: "enqueue_function",
          },
        ],
      });
    } finally {
      client.close();
      await served.close();
    }
  });
});

async function grpcUnary(
  client: ReturnType<typeof connect>,
  path: string,
  payload: unknown
) {
  const request = client.request({
    ":method": "POST",
    ":path": path,
    "content-type": "application/grpc",
  });
  const chunks: Buffer[] = [];
  request.on("data", (chunk) =>
    chunks.push(Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk))
  );
  const ended = once(request, "end");
  const failed = once(request, "error").then(([error]) => {
    throw error;
  });
  request.end(grpcFrame(payload));
  await Promise.race([ended, failed]);
  return readGrpcPayload(Buffer.concat(chunks));
}

function grpcFrame(payload: unknown) {
  const message = encodeJsonEnvelope(JSON.stringify(payload));
  const frame = Buffer.alloc(5 + message.length);
  frame[0] = 0;
  frame.writeUInt32BE(message.length, 1);
  message.copy(frame, 5);
  return frame;
}

function readGrpcPayload(body: Buffer) {
  const length = body.readUInt32BE(1);
  return JSON.parse(decodeJsonEnvelope(body.subarray(5, 5 + length)));
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
