import { describe, expect, test } from "vitest";

import {
  consolePackageKey,
  consolePackageRegistryByKey,
  installedConsolePackages,
} from "./console-package-registry";

describe("console package registry", () => {
  test("registers console packages by package export key", () => {
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
      consolePackageRegistryByKey()["@lenso/story-console#storyConsoleModule"]
        ?.module.id
    ).toBe("platform-story");
    expect(
      consolePackageRegistryByKey()[
        "@lenso/example-console#exampleConsoleModule"
      ]?.module.id
    ).toBe("example-console");
  });
});
