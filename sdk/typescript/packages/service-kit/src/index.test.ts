/* eslint-disable sort-keys */
import { describe, expect, it } from "vitest";

import {
  assertModuleContract,
  assertModuleRelease,
  assertServicePackage,
  assertServiceWorkspace,
  assertServiceContract,
  defineModuleContract,
  defineModuleRelease,
  defineKubernetesDeployment,
  defineServicePackage,
  defineServiceReleasePlan,
  defineServiceWorkspace,
  defineServiceContract,
  evaluateServiceReleasePolicy,
  moduleContractSchema,
  moduleReleaseSchema,
  serviceReleaseRestartRequired,
  serviceContractSchema,
  serviceEnv,
  serviceBaseUrlFromManifestUrl,
  serviceBaseUrlFromReadyUrl,
  serviceHealth,
  servicePackageSchema,
  serviceWorkspaceBaseUrl,
  serviceWorkspaceSchema,
  serviceWorkspaceToModuleServices,
  validateModuleContract,
  validateModuleRelease,
  validateServiceContract,
  validateServicePackage,
  validateServiceWorkspace,
} from "./index";

describe("defineServiceContract", () => {
  it("builds a provider service manifest with modules", () => {
    const manifest = defineServiceContract({
      compatibility: {
        providerProtocolVersion: "1",
        requiredHostFeatures: ["service.status"],
        sdkLanguage: "ts",
        sdkVersion: "0.1.0",
      },
      env: [serviceEnv("PORT", { example: "4110", required: true })],
      health: serviceHealth({
        readyUrl: "http://127.0.0.1:4110/lenso/service/v1/ready",
        statusUrl: "http://127.0.0.1:4110/lenso/service/v1/status",
      }),
      modules: [
        {
          name: "support-ticket",
          version: "0.1.0",
          capabilities: ["support_ticket.tickets.read"],
        },
      ],
      name: "support-suite-provider",
      provider: {
        name: "support-suite-provider",
        summary: "Support workflow provider",
        vendor: "Lenso",
      },
      version: "0.2.0",
    });

    expect(manifest).toMatchObject({
      env: [{ name: "PORT", required: true, example: "4110" }],
      health: {
        readyUrl: "http://127.0.0.1:4110/lenso/service/v1/ready",
      },
      modules: [{ name: "support-ticket" }],
      name: "support-suite-provider",
      provider: { vendor: "Lenso" },
    });
    expect(validateServiceContract(manifest)).toEqual([]);
    expect(() => assertServiceContract(manifest)).not.toThrow();
  });

  it("defaults omitted config and env without replacing provided fields", () => {
    const localProcess = {
      command: "pnpm start",
      env: { PORT: "4110" },
    };

    const manifest = defineServiceContract({
      localProcess,
      modules: [{ name: "support-ticket" }],
      name: "support-suite-provider",
    });

    expect(manifest.config).toEqual([]);
    expect(manifest.env).toEqual([]);
    expect(manifest.localProcess).toBe(localProcess);
  });

  it("exports the packaged service contract schema", () => {
    expect(serviceContractSchema.title).toBe("LensoServiceContract");
    expect(serviceContractSchema.required).toEqual(["name", "modules"]);
  });

  it("defines and validates a service package manifest", () => {
    const servicePackage = defineServicePackage({
      modules: ["support-ticket", "support-inbox"],
      name: "support-suite-provider",
      version: "0.2.0",
    });

    expect(servicePackage).toEqual({
      modules: ["support-ticket", "support-inbox"],
      name: "support-suite-provider",
      protocol: "lenso.service-package.v1",
      serviceManifest: "lenso.service.json",
      version: "0.2.0",
    });
    expect(servicePackageSchema.title).toBe("LensoServicePackage");
    expect(validateServicePackage(servicePackage)).toEqual([]);
    expect(() => assertServicePackage(servicePackage)).not.toThrow();
  });

  it("defines and validates a service workspace manifest", () => {
    const workspace = defineServiceWorkspace({
      services: [
        {
          command: "pnpm start",
          cwd: "services/support-suite-provider",
          lang: "ts",
          manifest: "lenso.service.json",
          modules: ["support-ticket"],
          name: "support-suite-provider",
          readyUrl: "http://127.0.0.1:4110/lenso/service/v1/status",
        },
      ],
    });

    expect(workspace).toEqual({
      protocol: "lenso.service-workspace.v1",
      services: [
        {
          autoStart: true,
          command: "pnpm start",
          cwd: "services/support-suite-provider",
          lang: "ts",
          manifest: "lenso.service.json",
          modules: ["support-ticket"],
          name: "support-suite-provider",
          readyTimeoutMs: 10_000,
          readyUrl: "http://127.0.0.1:4110/lenso/service/v1/status",
        },
      ],
    });
    expect(serviceWorkspaceSchema.title).toBe("LensoServiceWorkspace");
    expect(validateServiceWorkspace(workspace)).toEqual([]);
    expect(() => assertServiceWorkspace(workspace)).not.toThrow();
  });

  it("evaluates service release plans with delivery policy", () => {
    const diff = {
      capabilities: [
        {
          added: [],
          module: "support-ticket",
          removed: ["support_ticket.tickets.write"],
        },
      ],
      config: { added: ["support.mode"], removed: [] },
      env: { added: ["SUPPORT_API_KEY"], removed: [] },
      modules: { added: [], removed: [] },
      operations: [
        {
          added: [],
          module: "support-ticket",
          removed: ["route:DELETE /tickets/{id}"],
        },
      ],
    };

    expect(evaluateServiceReleasePolicy(diff).risk).toBe("breaking");
    expect(
      evaluateServiceReleasePolicy(diff, "remote protocol is newer").risk
    ).toBe("blocked");
    expect(serviceReleaseRestartRequired(diff)).toBe(true);

    const plan = defineServiceReleasePlan({
      candidate: {
        manifestReference: "./support/v2/lenso.service.json",
        modules: ["support-ticket"],
        name: "support-suite-provider",
        version: "0.2.0",
      },
      current: {
        manifestReference: "./support/v1/lenso.service.json",
        modules: ["support-ticket"],
        name: "support-suite-provider",
        version: "0.1.0",
      },
      diff,
      restartRequired: serviceReleaseRestartRequired(diff),
      service: { name: "support-suite-provider" },
    });

    expect(plan.protocol).toBe("lenso.service-release-plan.v1");
    expect(plan.policy.risk).toBe("breaking");
    expect(plan.nextAction).toContain("Review removed modules");
  });

  it("defines kubernetes deployment hints", () => {
    const deployment = defineKubernetesDeployment({
      ingressHost: "support-staging.example.com",
      port: 4110,
      replicas: 2,
      secrets: ["SUPPORT_TICKET_TOKEN"],
    });

    expect(deployment).toEqual({
      kubernetes: {
        ingressHost: "support-staging.example.com",
        port: 4110,
        replicas: 2,
        secrets: ["SUPPORT_TICKET_TOKEN"],
      },
      target: "kubernetes",
    });
  });

  it("converts service workspaces to module service start files", () => {
    const workspace = defineServiceWorkspace({
      services: [
        {
          command: "pnpm start",
          cwd: "services/support-suite-provider",
          lang: "ts",
          manifest: "lenso.service.json",
          modules: ["support-ticket"],
          name: "support-suite-provider",
          readyUrl: "http://127.0.0.1:4110/lenso/service/v1/status",
        },
      ],
    });

    expect(serviceWorkspaceToModuleServices(workspace)).toEqual({
      modules: [
        {
          moduleName: "support-suite-provider",
          services: [
            {
              autoStart: true,
              command: "pnpm start",
              cwd: "services/support-suite-provider",
              name: "support-suite-provider",
              readyTimeoutMs: 10_000,
              readyUrl: "http://127.0.0.1:4110/lenso/service/v1/status",
            },
          ],
        },
      ],
      version: 1,
    });
  });

  it("infers service base URLs from workspace service URLs", () => {
    expect(
      serviceBaseUrlFromReadyUrl(
        "http://127.0.0.1:4110/lenso/service/v1/status?probe=1"
      )
    ).toBe("http://127.0.0.1:4110/lenso/service/v1");
    expect(
      serviceBaseUrlFromManifestUrl(
        "http://127.0.0.1:4110/lenso/service/v1/manifest"
      )
    ).toBe("http://127.0.0.1:4110/lenso/service/v1");
    expect(
      serviceWorkspaceBaseUrl({
        manifest: "lenso.service.json",
        readyUrl: "http://127.0.0.1:4110/lenso/service/v1/ready",
      })
    ).toBe("http://127.0.0.1:4110/lenso/service/v1");
    expect(serviceBaseUrlFromReadyUrl("not a url")).toBeUndefined();
  });

  it("reports validation paths for malformed service workspaces", () => {
    const issues = validateServiceWorkspace({
      protocol: "lenso.workspace",
      services: [{ name: "", modules: ["support-ticket", 42] }],
    });

    expect(issues.map((issue) => issue.path)).toEqual([
      "$.protocol",
      "$.services[0].name",
      "$.services[0].lang",
      "$.services[0].cwd",
      "$.services[0].manifest",
      "$.services[0].command",
      "$.services[0].readyUrl",
      "$.services[0].modules[1]",
    ]);
  });

  it("reports validation paths for malformed service packages", () => {
    const issues = validateServicePackage({
      modules: ["support-ticket", "support-ticket", ""],
      name: "support-suite-provider",
      protocol: "service-module",
      serviceManifest: "lenso.service.json",
      version: "0.2.0",
    });

    expect(issues.map((issue) => issue.path)).toEqual([
      "$.protocol",
      "$.modules[1]",
      "$.modules[2]",
    ]);
  });

  it("defines and validates a module release manifest", () => {
    const release = defineModuleRelease({
      capabilities: ["support_ticket.tickets.read"],
      dependencies: ["auth"],
      name: "support-ticket",
      provider: { name: "support-suite-provider" },
      version: "0.2.0",
    });

    expect(release).toEqual({
      capabilities: ["support_ticket.tickets.read"],
      dependencies: ["auth"],
      name: "support-ticket",
      protocol: "lenso.module-release.v1",
      provider: {
        name: "support-suite-provider",
        servicePackage: "lenso.service-package.json",
      },
      source: "service",
      version: "0.2.0",
    });
    expect(moduleReleaseSchema.title).toBe("LensoModuleRelease");
    expect(validateModuleRelease(release)).toEqual([]);
    expect(() => assertModuleRelease(release)).not.toThrow();
  });

  it("defines and validates a linked module contract", () => {
    const contract = defineModuleContract({
      capabilities: ["support_ticket.tickets.read"],
      dependencies: ["auth"],
      manifest: { name: "support-ticket" },
      name: "support-ticket",
      source: "linked",
      version: "0.2.0",
    });

    expect(contract).toEqual({
      capabilities: ["support_ticket.tickets.read"],
      dependencies: ["auth"],
      manifest: { name: "support-ticket" },
      name: "support-ticket",
      protocol: "lenso.module.v1",
      source: "linked",
      version: "0.2.0",
    });
    expect(moduleContractSchema.title).toBe("LensoModuleContract");
    expect(validateModuleContract(contract)).toEqual([]);
    expect(() => assertModuleContract(contract)).not.toThrow();
  });

  it("allows linked module releases without service providers", () => {
    const release = defineModuleRelease({
      name: "auth-password",
      source: "linked",
      version: "0.2.0",
    });

    expect(release).toEqual({
      name: "auth-password",
      protocol: "lenso.module-release.v1",
      source: "linked",
      version: "0.2.0",
    });
    expect(validateModuleRelease(release)).toEqual([]);
  });

  it("keeps explicit module release service manifests", () => {
    const release = defineModuleRelease({
      name: "support-ticket",
      provider: {
        name: "support-suite-provider",
        serviceManifest: "https://example.test/lenso/service/v1/manifest",
      },
      version: "0.2.0",
    });

    expect(release.provider).toEqual({
      name: "support-suite-provider",
      serviceManifest: "https://example.test/lenso/service/v1/manifest",
    });
  });

  it("reports validation paths for malformed module releases", () => {
    const issues = validateModuleRelease({
      capabilities: ["support_ticket.read", 42],
      name: "",
      protocol: "service-module",
      provider: { name: "" },
      source: "service",
      version: "",
    });

    expect(issues.map((issue) => issue.path)).toEqual([
      "$.protocol",
      "$.name",
      "$.version",
      "$.provider.name",
      "$.provider.servicePackage",
      "$.capabilities[1]",
    ]);
  });

  it("reports validation paths for malformed module contracts", () => {
    const issues = validateModuleContract({
      capabilities: ["support_ticket.read", 42],
      manifest: [],
      name: "",
      protocol: "lenso.module",
      source: "service",
      version: "",
    });

    expect(issues.map((issue) => issue.path)).toEqual([
      "$.protocol",
      "$.name",
      "$.version",
      "$.capabilities[1]",
      "$.manifest",
    ]);
  });

  it("reports validation paths for malformed contracts", () => {
    const issues = validateServiceContract({
      install: {
        services: [{ name: "support-suite-provider" }],
      },
      modules: [
        {
          capabilities: ["support_ticket.read", 42],
          name: "support-ticket",
        },
        {
          name: "support-ticket",
        },
      ],
      name: "",
    });

    expect(issues.map((issue) => issue.path)).toEqual(
      expect.arrayContaining([
        "$.name",
        "$.install.services[0].command",
        "$.modules[0].capabilities[1]",
        "$.modules[1].name",
      ])
    );
  });
});
