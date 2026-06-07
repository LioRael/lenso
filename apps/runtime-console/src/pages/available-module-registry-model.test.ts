import { describe, expect, test } from "vitest";

import {
  type AvailableModuleRegistryDoctorSnapshot,
  type AvailableModuleRegistryCatalog,
  availableModuleRegistryRowsFromDoctorSnapshot,
  availableModuleRegistryRows,
} from "./available-module-registry-model";

const registryProvenance = {
  checksum: "sha256:fixture-billing-module",
  packageUrl: "https://example.com/lenso/module/v1/package.tgz",
  publisher: "Lenso Fixtures",
  publicKey:
    "-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAfixturekeyfixturekeyfixturekeyfi=\n-----END PUBLIC KEY-----\n",
  publicKeyId: "lenso-fixtures-ed25519",
  signatureAlgorithm: "ed25519-detached",
  signatureUrl: "https://example.com/lenso/module/v1/package.tgz.sig",
  sourceRepository: "https://example.com/lenso/billing-module",
};

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
      installPolicy: "trusted",
      provenance: registryProvenance,
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
        installPolicy: "trusted",
        name: "billing",
        preflightLabel: "unknown",
        preflightReason:
          "run lenso module registry doctor to fetch and validate manifest",
        preflightStatus: "unknown",
        provenanceChecksum: "sha256:fixture-billing-module",
        provenancePublisher: "Lenso Fixtures",
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

  test("marks untrusted catalog entries as review required", () => {
    expect(
      availableModuleRegistryRows({
        modules: [
          {
            baseUrl: "https://example.com/lenso/module/v1",
            manifestReference: "https://example.com/lenso/module/v1/manifest",
            name: "billing",
            source: "remote",
            version: "0.1.0",
          },
        ],
        version: 1,
      })[0]
    ).toMatchObject({
      installPolicy: "review_required",
      preflightLabel: "review required",
      preflightReason:
        "registry install requires installPolicy trusted after operator review",
      preflightStatus: "review_required",
    });
  });

  test("flags trusted entries with missing provenance", () => {
    expect(
      availableModuleRegistryRows({
        modules: [
          {
            baseUrl: "https://example.com/lenso/module/v1",
            installPolicy: "trusted",
            manifestReference: "https://example.com/lenso/module/v1/manifest",
            name: "billing",
            source: "remote",
            version: "0.1.0",
          },
        ],
        version: 1,
      })[0]
    ).toMatchObject({
      preflightLabel: "provenance required",
      preflightReason: "billing provenance publisher is missing",
      preflightStatus: "provenance_blocked",
    });
  });

  test("flags missing base url for local manifest references", () => {
    expect(
      availableModuleRegistryRows({
        modules: [
          {
            installPolicy: "trusted",
            provenance: registryProvenance,
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
      availableModuleRegistryRows({
        modules: [
          {
            baseUrl: "https://example.com/lenso/module/v1",
            compatibility: {
              lenso: {
                minVersion: "0.2.0",
              },
            },
            installPolicy: "trusted",
            provenance: registryProvenance,
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

  test("builds rows from registry doctor JSON snapshots", () => {
    const snapshot: AvailableModuleRegistryDoctorSnapshot = {
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
          installPolicy: "trusted",
          provenance: registryProvenance,
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
          installPolicy: "trusted",
          provenance: registryProvenance,
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

    expect(availableModuleRegistryRowsFromDoctorSnapshot(snapshot)).toEqual([
      {
        baseUrl: "https://example.com/lenso/module/v1",
        capabilityCount: 0,
        consolePackageHintCount: 1,
        key: "billing:0.1.0:https://example.com/lenso/module/v1/manifest",
        manifestReference: "https://example.com/lenso/module/v1/manifest",
        name: "billing",
        installPolicy: "trusted",
        preflightFix:
          "upgrade Lenso to 0.2.0 or install a compatible billing catalog entry",
        preflightLabel: "incompatible",
        preflightReason: "billing requires Lenso >= 0.2.0; host is 0.1.0",
        preflightStatus: "compatibility_blocked",
        provenanceChecksum: "sha256:fixture-billing-module",
        provenancePublisher: "Lenso Fixtures",
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
        installPolicy: "trusted",
        preflightLabel: "needs base URL",
        preflightFix: "add baseUrl or use a manifest URL ending with /manifest",
        preflightReason: "local-crm baseUrl is missing",
        preflightStatus: "needs_base_url",
        provenanceChecksum: "sha256:fixture-billing-module",
        provenancePublisher: "Lenso Fixtures",
        source: "remote",
        summary: "-",
        version: "0.1.0",
      },
    ]);
  });
});
