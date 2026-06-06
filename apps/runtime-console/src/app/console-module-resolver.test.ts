import { describe, expect, test } from "vitest";

import {
  resolveConsoleModule,
  resolveConsoleModules,
  selectConsoleModulePackageReferences,
} from "./console-module-resolver";

describe("console module resolver", () => {
  test("resolves first-party modules by package name and export name", () => {
    const module = resolveConsoleModule({
      exportName: "storyConsoleModule",
      packageName: "@lenso/story-console",
    });

    expect(module.id).toBe("platform-story");
  });

  test("builds the registry from package references", () => {
    const modules = resolveConsoleModules([
      {
        exportName: "storyConsoleModule",
        packageName: "@lenso/story-console",
      },
    ]);

    expect(modules.map((module) => module.id)).toEqual(["platform-story"]);
  });

  test("selects package references from backend module metadata", () => {
    expect(
      selectConsoleModulePackageReferences([
        {
          console: [
            {
              package: {
                export: "storyConsoleModule",
                name: "@lenso/story-console",
              },
            },
          ],
        },
        {
          console: [
            {
              package: {
                export: "unknownModule",
                name: "@lenso/unknown-console",
              },
            },
          ],
        },
      ])
    ).toEqual([
      {
        exportName: "storyConsoleModule",
        packageName: "@lenso/story-console",
      },
    ]);
  });

  test("reports missing package exports with the package reference", () => {
    expect(() =>
      resolveConsoleModule({
        exportName: "missingExport",
        packageName: "@lenso/story-console",
      })
    ).toThrow(
      "Console module package export is not registered: @lenso/story-console#missingExport"
    );
  });
});
