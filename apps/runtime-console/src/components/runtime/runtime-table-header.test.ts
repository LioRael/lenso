import { describe, expect, test } from "vitest";

import {
  runtimeTableHeaderBaseClassName,
  runtimeTimelineTableHeaderClassName,
  runtimeWaterfallTableHeaderClassName,
} from "./runtime-table-header";

describe("runtime table header style contract", () => {
  test("keeps timeline and waterfall table headers on the same strip style", () => {
    expect(runtimeTableHeaderBaseClassName).toContain("border-b");
    expect(runtimeTableHeaderBaseClassName).toContain("bg-[color-mix");
    expect(runtimeTableHeaderBaseClassName).toContain("px-3");
    expect(runtimeTableHeaderBaseClassName).toContain("py-2");
    expect(runtimeTimelineTableHeaderClassName).toContain(
      runtimeTableHeaderBaseClassName
    );
    expect(runtimeWaterfallTableHeaderClassName).toContain(
      runtimeTableHeaderBaseClassName
    );
  });
});
