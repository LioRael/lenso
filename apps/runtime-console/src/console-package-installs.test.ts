import { describe, expect, test } from "vitest";

import {
  consolePackageKey,
  consolePackageRegistryByKey,
} from "./app/console-package-registry";
import { consolePackageInstallManifests } from "./console-package-install-manifests";
import { installedConsolePackages } from "./console-package-installs";
import {
  consolePackageManifests,
  consolePackageNames,
} from "./console-package-manifest-exports";
import { consolePackageModuleExportsByKey } from "./console-package-module-exports";

const installsSource =
  Object.values(
    import.meta.glob<string>("./console-package-installs.ts", {
      eager: true,
      import: "default",
      query: "?raw",
    })
  )[0] ?? "";
const installManifestsSource =
  Object.values(
    import.meta.glob<string>("./console-package-install-manifests.ts", {
      eager: true,
      import: "default",
      query: "?raw",
    })
  )[0] ?? "";
const runtimeConsolePackageJson =
  Object.values(
    import.meta.glob<{ dependencies?: Record<string, string> }>(
      "../package.json",
      {
        eager: true,
        import: "default",
      }
    )
  )[0] ?? {};

describe("console package installs", () => {
  test("keeps concrete package imports in install manifests and module mappings", () => {
    expect(installsSource).not.toContain("@lenso/story-console");
    expect(installsSource).not.toContain("@lenso/example-console");
    expect(installsSource).not.toContain("storyConsoleModule");
    expect(installsSource).not.toContain("exampleConsoleModule");
  });

  test("registers installed workspace console packages", () => {
    expect(consolePackageInstallManifests).toHaveLength(2);
    expect(Object.keys(consolePackageModuleExportsByKey)).toEqual([
      "@lenso/example-console#exampleConsoleModule",
      "@lenso/story-console#storyConsoleModule",
    ]);
    expect(installedConsolePackages).toMatchObject([
      {
        exportName: "exampleConsoleModule",
        packageName: "@lenso/example-console",
        source: "installed",
        version: "workspace",
      },
      {
        exportName: "storyConsoleModule",
        packageName: "@lenso/story-console",
        source: "first_party",
        version: "workspace",
      },
    ]);
    expect(consolePackageKey(installedConsolePackages[0]!)).toBe(
      "@lenso/example-console#exampleConsoleModule"
    );
    expect(
      consolePackageRegistryByKey(installedConsolePackages)[
        "@lenso/story-console#storyConsoleModule"
      ]?.module.id
    ).toBe("platform-story");
    expect(
      consolePackageRegistryByKey(installedConsolePackages)[
        "@lenso/example-console#exampleConsoleModule"
      ]?.module.id
    ).toBe("example-console");
  });

  test("derives install manifests from the package manifest export list", () => {
    expect(installManifestsSource).not.toContain("@lenso/story-console");
    expect(installManifestsSource).not.toContain("@lenso/example-console");
    expect(consolePackageInstallManifests.map((item) => item.manifest)).toEqual(
      consolePackageManifests
    );
  });

  test("keeps installed package manifests aligned with host dependencies", () => {
    const dependencyNames = Object.keys(
      runtimeConsolePackageJson.dependencies ?? {}
    ).filter(
      (name) =>
        name !== "@lenso/runtime-console-api" && name.startsWith("@lenso/")
    );

    expect(consolePackageNames).toEqual(dependencyNames);
  });

  test("keeps module export mapping aligned with install manifests", () => {
    expect(Object.keys(consolePackageModuleExportsByKey)).toEqual(
      consolePackageInstallManifests.map((item) =>
        consolePackageKey(item.manifest)
      )
    );
  });
});
