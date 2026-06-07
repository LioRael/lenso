import { describe, expect, test } from "vitest";

import {
  type AvailableModuleRegistryCatalog,
  availableModuleRegistryRows,
} from "./available-module-registry-model";

const catalog: AvailableModuleRegistryCatalog = {
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

describe("available module registry model", () => {
  test("builds rows from registry catalog entries", () => {
    expect(availableModuleRegistryRows(catalog)).toEqual([
      {
        baseUrl: "https://example.com/lenso/module/v1",
        capabilityCount: 2,
        consolePackageHintCount: 1,
        key: "billing:0.1.0:https://example.com/lenso/module/v1/manifest",
        manifestReference: "https://example.com/lenso/module/v1/manifest",
        name: "billing",
        preflightLabel: "unknown",
        preflightReason:
          "run lenso module registry doctor to fetch and validate manifest",
        preflightStatus: "unknown",
        source: "remote",
        summary: "Billing workspace and operations",
        version: "0.1.0",
      },
    ]);
  });

  test("marks entries ready when the manifest snapshot matches", () => {
    expect(
      availableModuleRegistryRows(catalog, {
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
      preflightReason:
        "catalog entry matches the fetched remote manifest snapshot",
      preflightStatus: "ready",
    });
  });

  test("flags missing base url for local manifest references", () => {
    expect(
      availableModuleRegistryRows({
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

  test("flags manifest identity mismatches", () => {
    expect(
      availableModuleRegistryRows(catalog, {
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
      availableModuleRegistryRows(catalog, {
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
});
