import { describe, expect, test } from "vitest";

import {
  traceViewHeaderClassName,
  traceViewHeaderContentClassName,
  traceViewHeaderLabelClassName,
  traceViewHeaderMetaClassName,
  traceViewHeaderSummaryClassName,
} from "./trace-view-header";

describe("trace view header style contract", () => {
  test("uses one shared style contract for trace tab titles", () => {
    expect(traceViewHeaderClassName).toContain("border-b");
    expect(traceViewHeaderClassName).toContain("bg-[color-mix");
    expect(traceViewHeaderContentClassName).toContain("overflow-hidden");
    expect(traceViewHeaderLabelClassName).toContain("uppercase");
    expect(traceViewHeaderSummaryClassName).toContain("truncate");
    expect(traceViewHeaderMetaClassName).toContain("shrink-0");
  });
});
