import { describe, expect, test } from "vitest";

import {
  consolePackageInstallPlanFromMetadata,
  consoleModuleMetadataWithFallback,
  missingConsolePackagesFromMetadata,
  navigationFromConsoleModuleMetadata,
  previewConsolePackageInstallResults,
} from "./console-module-metadata";

describe("console module metadata", () => {
  test("builds navigation from backend console metadata", () => {
    expect(
      navigationFromConsoleModuleMetadata(
        [
          {
            console: [
              {
                navigation: {
                  order: 20,
                  workspace: {
                    icon: "settings",
                    id: "system",
                    label: "System",
                  },
                },
                package: {
                  export: "storyConsoleModule",
                  name: "@lenso/story-console",
                },
                required_capabilities: ["runtime.stories.read"],
              },
            ],
          },
        ],
        ["runtime.stories.read", "identity.users.read"]
      )
    ).toEqual([
      {
        icon: "workflow",
        label: "Stories",
        moduleId: "platform-story",
        navigation: {
          order: 20,
          workspace: {
            icon: "settings",
            id: "system",
            label: "System",
          },
        },
        path: "/runtime/stories",
      },
    ]);
  });

  test("omits navigation when required capabilities are unavailable", () => {
    expect(
      navigationFromConsoleModuleMetadata(
        [
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
        ],
        []
      )
    ).toEqual([]);
  });

  test("reports missing console packages from metadata", () => {
    expect(
      missingConsolePackagesFromMetadata([
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
              route: "/data/crm",
            },
          ],
        },
      ]).map((row) => row.key)
    ).toEqual(["@lenso/crm-console#crmConsoleModule"]);
  });

  test("builds console package install plans from metadata", () => {
    expect(
      consolePackageInstallPlanFromMetadata([
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
              route: "/data/crm",
            },
          ],
        },
      ])
    ).toEqual([
      {
        exportName: "crmConsoleModule",
        key: "@lenso/crm-console#crmConsoleModule",
        packageName: "@lenso/crm-console",
        reason: "remote-crm / CRM / /data/crm",
        request: {
          exportName: "crmConsoleModule",
          packageName: "@lenso/crm-console",
          requestedByModule: "remote-crm",
          route: "/data/crm",
        },
        status: "planned",
      },
    ]);
  });

  test("previews manual install results from metadata", async () => {
    await expect(
      previewConsolePackageInstallResults([
        {
          module_name: "remote-crm",
          console: [
            {
              package: {
                export: "crmConsoleModule",
                name: "@lenso/crm-console",
              },
              route: "/data/crm",
            },
          ],
        },
      ])
    ).resolves.toEqual([
      {
        command: "pnpm --dir apps/runtime-console add @lenso/crm-console",
        exportName: "crmConsoleModule",
        key: "@lenso/crm-console#crmConsoleModule",
        message: "manual dev install required",
        packageName: "@lenso/crm-console",
        request: {
          exportName: "crmConsoleModule",
          packageName: "@lenso/crm-console",
          requestedByModule: "remote-crm",
          route: "/data/crm",
        },
        status: "requires_manual_install",
      },
    ]);
  });

  test("uses backend metadata when it is available", () => {
    const backendMetadata = [{ console: [] }];

    expect(
      consoleModuleMetadataWithFallback({
        apiMode: true,
        data: backendMetadata,
        isError: false,
        isPending: false,
      })
    ).toBe(backendMetadata);
  });

  test("falls back while metadata is loading or unavailable", () => {
    expect(
      navigationFromConsoleModuleMetadata(
        consoleModuleMetadataWithFallback({
          apiMode: true,
          data: undefined,
          isError: false,
          isPending: true,
        }),
        ["runtime.stories.read", "identity.users.read"]
      ).map((item) => item.path)
    ).toEqual(["/runtime/stories", "/data/identity"]);

    expect(
      navigationFromConsoleModuleMetadata(
        consoleModuleMetadataWithFallback({
          apiMode: false,
          data: undefined,
          isError: false,
          isPending: false,
        }),
        ["runtime.stories.read", "identity.users.read"]
      ).map((item) => item.path)
    ).toEqual(["/runtime/stories", "/data/identity"]);
  });
});
