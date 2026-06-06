import { describe, expect, test } from "vitest";

import {
  missingConsolePackageReferences,
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
              required_capabilities: ["runtime.stories.read"],
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

  test("filters console surfaces when required capabilities are missing", () => {
    const metadata = [
      {
        console: [
          {
            package: {
              export: "storyConsoleModule",
              name: "@lenso/story-console",
            },
            required_capabilities: ["runtime.stories.read"],
          },
        ],
      },
    ];

    expect(
      selectConsoleModulePackageReferences(metadata, {
        availableCapabilities: [],
      })
    ).toEqual([]);
    expect(
      selectConsoleModulePackageReferences(metadata, {
        availableCapabilities: ["runtime.stories.read"],
      })
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

  test("collects unsupported package references for installation planning", () => {
    expect(
      missingConsolePackageReferences([
        {
          module_name: "remote-crm",
          console: [
            {
              label: "CRM",
              name: "crm",
              package: {
                export: "crmConsoleModule",
                name: "@lenso/crm-console",
              },
              required_capabilities: ["remote_crm.contacts.read"],
              route: "/data/crm",
            },
            {
              label: "Stories",
              name: "stories",
              package: {
                export: "storyConsoleModule",
                name: "@lenso/story-console",
              },
              route: "/runtime/stories",
            },
          ],
        },
      ])
    ).toEqual([
      {
        exportName: "crmConsoleModule",
        key: "@lenso/crm-console#crmConsoleModule",
        moduleName: "remote-crm",
        packageName: "@lenso/crm-console",
        requiredCapabilities: ["remote_crm.contacts.read"],
        route: "/data/crm",
        surfaceLabel: "CRM",
        surfaceName: "crm",
      },
    ]);
  });
});
