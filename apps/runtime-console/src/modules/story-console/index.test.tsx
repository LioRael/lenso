import { describe, expect, test } from "vitest";

import {
  resizeStoryListWidth,
  RuntimeStoriesPage,
  resolveSelectedRuntimeStory,
  runtimeStoriesPath,
  shouldCloseInspectorOnEscape,
  storyConsoleManifest,
  storyConsoleModule,
} from ".";

describe("story console module", () => {
  test("exports the Stories route as a first-party console module", () => {
    expect(storyConsoleManifest).toEqual({
      area: "runtime",
      exportName: "storyConsoleModule",
      icon: "workflow",
      id: "platform-story",
      label: "Stories",
      packageName: "@lenso/story-console",
      requiredCapabilities: ["runtime.stories.read"],
      route: "/runtime/stories",
      surfaceName: "stories",
    });
    expect(storyConsoleModule).toMatchObject({
      id: storyConsoleManifest.id,
      surfaces: [
        {
          area: storyConsoleManifest.area,
          icon: storyConsoleManifest.icon,
          label: storyConsoleManifest.label,
          path: storyConsoleManifest.route,
        },
      ],
    });
    expect(storyConsoleModule.surfaces[0]?.component).toBeTypeOf("function");
    expect(storyConsoleModule.surfaces[0]?.component).toBe(RuntimeStoriesPage);
  });

  test("exports story model helpers from the module boundary", () => {
    expect(runtimeStoriesPath()).toBe("/runtime/stories");
    expect(resizeStoryListWidth(300, 20)).toBe(320);
    expect(resolveSelectedRuntimeStory([], null, true)).toBeNull();
    expect(
      shouldCloseInspectorOnEscape({
        altKey: false,
        ctrlKey: false,
        defaultPrevented: false,
        key: "Escape",
        metaKey: false,
        target: null,
      })
    ).toBe(true);
  });
});
