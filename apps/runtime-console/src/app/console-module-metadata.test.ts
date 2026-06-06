import { describe, expect, test } from "vitest";

import {
  consoleModuleMetadataWithFallback,
  navigationFromConsoleModuleMetadata,
} from "./console-module-metadata";

describe("console module metadata", () => {
  test("builds navigation from backend console metadata", () => {
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
        ["runtime.stories.read"]
      )
    ).toEqual([
      {
        icon: "workflow",
        label: "Stories",
        moduleId: "platform-story",
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
        ["runtime.stories.read"]
      ).map((item) => item.path)
    ).toEqual(["/runtime/stories"]);

    expect(
      navigationFromConsoleModuleMetadata(
        consoleModuleMetadataWithFallback({
          apiMode: false,
          data: undefined,
          isError: false,
          isPending: false,
        }),
        ["runtime.stories.read"]
      ).map((item) => item.path)
    ).toEqual(["/runtime/stories"]);
  });
});
