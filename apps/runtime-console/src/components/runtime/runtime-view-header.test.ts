import { describe, expect, test } from "vitest";

import {
  runtimeViewHeaderClassName,
  runtimeViewHeaderContentClassName,
  runtimeViewHeaderLabelClassName,
  runtimeViewHeaderMetaClassName,
  runtimeViewHeaderSummaryClassName,
} from "./runtime-view-header";

describe("runtime view header style contract", () => {
  test("uses one shared style contract for runtime tab titles", () => {
    expect(runtimeViewHeaderClassName).toContain("border-b");
    expect(runtimeViewHeaderClassName).toContain("bg-[color-mix");
    expect(runtimeViewHeaderContentClassName).toContain("overflow-hidden");
    expect(runtimeViewHeaderLabelClassName).toContain("uppercase");
    expect(runtimeViewHeaderSummaryClassName).toContain("truncate");
    expect(runtimeViewHeaderMetaClassName).toContain("shrink-0");
  });
});
