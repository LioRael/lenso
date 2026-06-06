import { describe, expect, test } from "vitest";

import {
  consolePackageKey,
  consolePackageRegistryByKey,
  defineInstalledConsolePackage,
  resolveInstalledConsolePackages,
} from "./console-package-registry";

const registrySource =
  Object.values(
    import.meta.glob<string>("./console-package-registry.ts", {
      eager: true,
      import: "default",
      query: "?raw",
    })
  )[0] ?? "";

describe("console package registry", () => {
  test("keeps concrete package installs outside the registry implementation", () => {
    expect(registrySource).not.toContain("@lenso/story-console");
    expect(registrySource).not.toContain("@lenso/example-console");
    expect(registrySource).not.toContain("installedConsolePackages");
  });

  test("registers console packages by package export key", () => {
    const installedPackage = defineInstalledConsolePackage({
      manifest: {
        exportName: "billingConsoleModule",
        packageName: "@lenso/billing-console",
      },
      module: {
        id: "billing",
        surfaces: [],
      },
      source: "installed",
      version: "workspace",
    });

    expect(installedPackage).toMatchObject({
      exportName: "billingConsoleModule",
      packageName: "@lenso/billing-console",
      source: "installed",
      version: "workspace",
    });
    expect(consolePackageKey(installedPackage)).toBe(
      "@lenso/billing-console#billingConsoleModule"
    );
    expect(
      consolePackageRegistryByKey([installedPackage])[
        "@lenso/billing-console#billingConsoleModule"
      ]?.module.id
    ).toBe("billing");
  });

  test("resolves installed packages from declarations and module exports", () => {
    const installedPackages = resolveInstalledConsolePackages(
      [
        {
          manifest: {
            exportName: "billingConsoleModule",
            packageName: "@lenso/billing-console",
          },
          source: "installed",
          version: "workspace",
        },
      ],
      {
        "@lenso/billing-console#billingConsoleModule": {
          id: "billing",
          surfaces: [],
        },
      }
    );

    expect(installedPackages[0]?.module.id).toBe("billing");
  });

  test("rejects install declarations without a module export", () => {
    expect(() =>
      resolveInstalledConsolePackages(
        [
          {
            manifest: {
              exportName: "missingConsoleModule",
              packageName: "@lenso/missing-console",
            },
            source: "installed",
          },
        ],
        {}
      )
    ).toThrow(
      "Console package module export is not installed: @lenso/missing-console#missingConsoleModule"
    );
  });
});
