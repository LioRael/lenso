import { describe, expect, test } from "vitest";

import {
  consolePackageKey,
  consolePackageRegistryByKey,
  defineInstalledConsolePackage,
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
});
