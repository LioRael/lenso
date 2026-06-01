import { describe, expect, test } from "vitest";

import {
  traceTableHeaderBaseClassName,
  traceTimelineTableHeaderClassName,
  traceWaterfallTableHeaderClassName,
} from "./trace-table-header";

describe("trace table header style contract", () => {
  test("keeps timeline and waterfall table headers on the same strip style", () => {
    expect(traceTableHeaderBaseClassName).toContain("border-b");
    expect(traceTableHeaderBaseClassName).toContain("bg-[color-mix");
    expect(traceTableHeaderBaseClassName).toContain("px-3");
    expect(traceTableHeaderBaseClassName).toContain("py-2");
    expect(traceTimelineTableHeaderClassName).toContain(
      traceTableHeaderBaseClassName
    );
    expect(traceWaterfallTableHeaderClassName).toContain(
      traceTableHeaderBaseClassName
    );
  });
});
