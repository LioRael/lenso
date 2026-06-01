import { describe, expect, test } from "vitest";

import type { TraceHeader } from "../components/runtime/trace-header";
import { runtimeStories } from "../data/mock-runtime";
import { storyWorkbenchDefaultViewMode } from "./trace-workbench-page";

describe("story workbench page contracts", () => {
  test("defaults to the runtime story visualization mode", () => {
    const defaultViewMode: "story" = storyWorkbenchDefaultViewMode;

    expect(defaultViewMode).toBe("story");
  });

  test("keeps story header props aligned with story data", () => {
    const story = runtimeStories[0]!;
    const traceHeaderProps: Parameters<typeof TraceHeader>[0] = {
      onClose: () => undefined,
      onSelectNode: () => undefined,
      story,
    };

    expect(traceHeaderProps.story.id).toBe(story.id);
  });
});
