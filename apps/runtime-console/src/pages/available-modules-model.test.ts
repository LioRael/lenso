import { describe, expect, test } from "vitest";

import {
  type AvailableModulesResponse,
  type AvailableModulesCatalog,
  availableModuleRowsFromResponse,
  availableModuleRows,
} from "./available-modules-model";

const catalog: AvailableModulesCatalog = {
  modules: [
    {
      baseUrl: "https://example.com/lenso/module/v1",
      capabilities: ["billing.read", "billing.write"],
      consolePackages: [
        {
          exportName: "billingConsoleModule",
          packageName: "@vendor/lenso-billing-console",
          route: "/data/billing",
        },
      ],
      manifestReference: "https://example.com/lenso/module/v1/manifest",
      name: "billing",
      source: "remote",
      summary: "Billing workspace and operations",
      version: "0.1.0",
    },
  ],
  version: 1,
};

describe("available modules model", () => {
  test("builds rows from available module catalog entries", () => {
    expect(availableModuleRows(catalog)).toEqual([
      {
        baseUrl: "https://example.com/lenso/module/v1",
        capabilityCount: 2,
        consolePackageHintCount: 1,
        key: "billing:0.1.0:https://example.com/lenso/module/v1/manifest",
        manifestReference: "https://example.com/lenso/module/v1/manifest",
        name: "billing",
        preflightLabel: "unknown",
        preflightReason:
          "manifest will be read from the manifest URL during install",
        preflightStatus: "unknown",
        source: "remote",
        summary: "Billing workspace and operations",
        version: "0.1.0",
      },
    ]);
  });

  test("marks entries ready when the manifest snapshot matches", () => {
    expect(
      availableModuleRows(catalog, {
        billing: {
          consolePackages: [
            {
              exportName: "billingConsoleModule",
              packageName: "@vendor/lenso-billing-console",
            },
          ],
          name: "billing",
          source: "remote",
          version: "0.1.0",
        },
      })[0]
    ).toMatchObject({
      preflightLabel: "ready",
      preflightReason: "module manifest is available",
      preflightStatus: "ready",
    });
  });

  test("flags missing base url for local manifest references", () => {
    expect(
      availableModuleRows({
        modules: [
          {
            manifestReference: "./lenso.module.json",
            name: "billing",
            source: "remote",
            version: "0.1.0",
          },
        ],
        version: 1,
      })[0]
    ).toMatchObject({
      preflightLabel: "needs base URL",
      preflightStatus: "needs_base_url",
    });
  });

  test("flags incompatible catalog entries before manifest checks", () => {
    expect(
      availableModuleRows({
        modules: [
          {
            baseUrl: "https://example.com/lenso/module/v1",
            compatibility: {
              lenso: {
                minVersion: "0.2.0",
              },
            },
            manifestReference: "https://example.com/lenso/module/v1/manifest",
            name: "billing",
            source: "remote",
            version: "0.1.0",
          },
        ],
        version: 1,
      })[0]
    ).toMatchObject({
      preflightLabel: "incompatible",
      preflightReason: "billing requires Lenso >= 0.2.0; host is 0.1.0",
      preflightStatus: "compatibility_blocked",
    });
  });

  test("flags manifest identity mismatches", () => {
    expect(
      availableModuleRows(catalog, {
        billing: {
          name: "billing-pro",
          source: "remote",
          version: "0.2.0",
        },
      })[0]
    ).toMatchObject({
      preflightLabel: "manifest mismatch",
      preflightStatus: "manifest_mismatch",
    });
  });

  test("flags console package hint mismatches", () => {
    expect(
      availableModuleRows(catalog, {
        billing: {
          consolePackages: [
            {
              exportName: "crmConsoleModule",
              packageName: "@vendor/lenso-crm-console",
            },
          ],
          name: "billing",
          source: "remote",
          version: "0.1.0",
        },
      })[0]
    ).toMatchObject({
      preflightLabel: "package hint mismatch",
      preflightStatus: "package_hint_mismatch",
    });
  });

  test("builds rows from available module responses", () => {
    const response: AvailableModulesResponse = {
      catalog: {
        modules: 2,
        registryFile: ".lenso/module-registry.json",
        version: 1,
      },
      issues: [
        {
          fix: "add baseUrl or use a manifest URL ending with /manifest",
          group: "Catalog",
          message: "local-crm baseUrl is missing",
        },
        {
          fix: "upgrade Lenso to 0.2.0 or install a compatible billing catalog entry",
          group: "Compatibility",
          message: "billing requires Lenso >= 0.2.0; host is 0.1.0",
        },
      ],
      modules: [
        {
          baseUrl: "https://example.com/lenso/module/v1",
          catalogVersion: "0.1.0",
          consolePackageHints: 1,
          compatibility: {
            lenso: {
              minVersion: "0.2.0",
            },
          },
          hostCompatibility: {
            consolePackageApi: "1",
            lensoVersion: "0.1.0",
          },
          manifestName: "billing",
          manifestReference: "https://example.com/lenso/module/v1/manifest",
          manifestStatus: "ok",
          manifestVersion: "0.1.0",
          name: "billing",
          source: "remote",
          status: "needs_attention",
        },
        {
          baseUrl: null,
          catalogVersion: "0.1.0",
          consolePackageHints: 0,
          manifestName: "local-crm",
          manifestReference: "./lenso.module.json",
          manifestStatus: "ok",
          manifestVersion: "0.1.0",
          name: "local-crm",
          source: "remote",
          status: "needs_attention",
        },
      ],
      status: "failed",
      version: 1,
    };

    expect(availableModuleRowsFromResponse(response)).toEqual([
      {
        baseUrl: "https://example.com/lenso/module/v1",
        capabilityCount: 0,
        consolePackageHintCount: 1,
        key: "billing:0.1.0:https://example.com/lenso/module/v1/manifest",
        manifestReference: "https://example.com/lenso/module/v1/manifest",
        name: "billing",
        preflightFix:
          "upgrade Lenso to 0.2.0 or install a compatible billing catalog entry",
        preflightLabel: "incompatible",
        preflightReason: "billing requires Lenso >= 0.2.0; host is 0.1.0",
        preflightStatus: "compatibility_blocked",
        source: "remote",
        summary: "-",
        version: "0.1.0",
      },
      {
        baseUrl: "-",
        capabilityCount: 0,
        consolePackageHintCount: 0,
        key: "local-crm:0.1.0:./lenso.module.json",
        manifestReference: "./lenso.module.json",
        name: "local-crm",
        preflightLabel: "needs base URL",
        preflightFix: "add baseUrl or use a manifest URL ending with /manifest",
        preflightReason: "local-crm baseUrl is missing",
        preflightStatus: "needs_base_url",
        source: "remote",
        summary: "-",
        version: "0.1.0",
      },
    ]);
  });

  test("marks archived available modules without restore handoff", () => {
    const response: AvailableModulesResponse = {
      catalog: {
        modules: 1,
        registryFile: ".lenso/module-registry.json",
        version: 1,
      },
      issues: [],
      modules: [
        {
          archivedAt: "2026-06-07T12:00:00.000Z",
          archiveReason: "replaced by billing-v2",
          baseUrl: "https://example.com/lenso/module/v1",
          catalogVersion: "0.1.0",
          consolePackageHints: 1,
          manifestName: null,
          manifestReference: "https://example.com/lenso/module/v1/manifest",
          manifestStatus: "archived",
          manifestVersion: null,
          name: "billing",
          source: "remote",
          status: "archived",
        },
      ],
      status: "passed",
      version: 1,
    };

    expect(availableModuleRowsFromResponse(response)[0]).toMatchObject({
      preflightLabel: "archived",
      preflightReason: "catalog entry archived: replaced by billing-v2",
      preflightStatus: "archived",
    });
  });
});
