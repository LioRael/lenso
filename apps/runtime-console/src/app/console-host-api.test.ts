import { describe, expect, test } from "vitest";

import { runtimeConsoleHostApi } from "./console-host-api";

describe("console host api", () => {
  test("exposes stable host capabilities consumed by console modules", () => {
    expect(runtimeConsoleHostApi.queries.useRuntimeStories).toBeTypeOf(
      "function"
    );
    expect(runtimeConsoleHostApi.adminData.useRecords).toBeTypeOf("function");
    expect(runtimeConsoleHostApi.context.useRuntimeConsole).toBeTypeOf(
      "function"
    );
    expect(runtimeConsoleHostApi.hooks.useBrowserUrlPopState).toBeTypeOf(
      "function"
    );
    expect(runtimeConsoleHostApi.hooks.useListKeyboard).toBeTypeOf("function");
    expect(runtimeConsoleHostApi.hooks.usePersistedLayout).toBeTypeOf(
      "function"
    );
    expect(runtimeConsoleHostApi.hooks.writeBrowserUrl).toBeTypeOf("function");
    expect(runtimeConsoleHostApi.data.retryTargetForNode).toBeTypeOf(
      "function"
    );
    expect(runtimeConsoleHostApi.story.findStoryByCorrelation).toBeTypeOf(
      "function"
    );
    expect(runtimeConsoleHostApi.story.executionInspectorTabs.length).toBe(7);
    expect(runtimeConsoleHostApi.ui.runtime.ExecutionInspector).toBeTypeOf(
      "function"
    );
    expect(runtimeConsoleHostApi.ui.runtime.ResizeHandle).toBeTypeOf(
      "function"
    );
    expect(runtimeConsoleHostApi.ui.runtime.StoryList).toBeTypeOf("function");
    expect(runtimeConsoleHostApi.ui.common.EmptyState).toBeTypeOf("function");
    expect(
      runtimeConsoleHostApi.routing.buildPath("/runtime/stories", {
        story: "corr_1",
      })
    ).toBe("/runtime/stories?story=corr_1");
  });
});
