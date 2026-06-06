import { describe, expect, test } from "vitest";

import {
  consolePackageKey,
  consolePackageRegistryByKey,
} from "./app/console-package-registry";
import { consolePackageInstallManifests } from "./console-package-install-manifests";
import { installedConsolePackages } from "./console-package-installs";
import { consolePackageModuleExportsByKey } from "./console-package-module-exports";

const installsSource =
  Object.values(
    import.meta.glob<string>("./console-package-installs.ts", {
      eager: true,
      import: "default",
      query: "?raw",
    })
  )[0] ?? "";

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
      "@lenso/story-console#storyConsoleModule",
      "@lenso/example-console#exampleConsoleModule",
    ]);
    expect(installedConsolePackages).toMatchObject([
      {
        exportName: "storyConsoleModule",
        packageName: "@lenso/story-console",
        source: "first_party",
        version: "workspace",
      },
      {
        exportName: "exampleConsoleModule",
        packageName: "@lenso/example-console",
        source: "installed",
        version: "workspace",
      },
    ]);
    expect(consolePackageKey(installedConsolePackages[0]!)).toBe(
      "@lenso/story-console#storyConsoleModule"
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
});
