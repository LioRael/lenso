import type { RuntimeStory } from "@lenso/runtime-console/console-package-api";
import { describe, expect, test } from "vitest";

import { resolveSelectedRuntimeStory } from "./selection";

const stories = [
  { correlationId: "corr_a", id: "corr_a", name: "A" },
  { correlationId: "corr_b", id: "corr_b", name: "B" },
] as RuntimeStory[];

describe("runtime story selection", () => {
  test("persists selected story by correlation id", () => {
    expect(resolveSelectedRuntimeStory(stories, "corr_b", false)?.name).toBe(
      "B"
    );
  });

  test("falls back to first visible story when persisted story is missing", () => {
    expect(resolveSelectedRuntimeStory(stories, "missing", false)?.name).toBe(
      "A"
    );
  });

  test("keeps detail closed until the user selects another story", () => {
    expect(resolveSelectedRuntimeStory(stories, "corr_a", true)).toBeNull();
  });
});
