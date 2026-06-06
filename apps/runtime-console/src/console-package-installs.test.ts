import { describe, expect, test } from "vitest";

import {
  consolePackageKey,
  consolePackageRegistryByKey,
} from "./app/console-package-registry";
import { installedConsolePackages } from "./console-package-installs";

describe("console package installs", () => {
  test("registers installed workspace console packages", () => {
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
