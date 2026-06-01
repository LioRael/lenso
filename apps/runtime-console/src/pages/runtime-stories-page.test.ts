import { describe, expect, test } from "vitest";

import type { StoryHeader } from "../components/runtime/story-header";
import { runtimeStories } from "../data/mock-runtime";
import { runtimeStoriesDefaultViewMode } from "./runtime-stories-page";

describe("story workbench page contracts", () => {
  test("defaults to the runtime story visualization mode", () => {
    const defaultViewMode: "story" = runtimeStoriesDefaultViewMode;

    expect(defaultViewMode).toBe("story");
  });

  test("keeps story header props aligned with story data", () => {
    const story = runtimeStories[0]!;
    const storyHeaderProps: Parameters<typeof StoryHeader>[0] = {
      onClose: () => undefined,
      onSelectNode: () => undefined,
      story,
    };

    expect(storyHeaderProps.story.id).toBe(story.id);
  });
});
