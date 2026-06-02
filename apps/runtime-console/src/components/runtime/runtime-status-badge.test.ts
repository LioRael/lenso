import { describe, expect, test } from "vitest";

import {
  runtimeStatusBadgeBaseClassName,
  runtimeStatusBadgeLabelClassName,
  runtimeStatusBadgeTableClassName,
} from "./runtime-status-badge";

describe("runtime status badge style contract", () => {
  test("keeps the label variant aligned with inspector header labels", () => {
    expect(runtimeStatusBadgeBaseClassName).toContain("rounded-xs");
    expect(runtimeStatusBadgeBaseClassName).toContain("font-mono");
    expect(runtimeStatusBadgeBaseClassName).toContain("uppercase");
    expect(runtimeStatusBadgeLabelClassName).toContain("py-0.5");
    expect(runtimeStatusBadgeLabelClassName).toContain("tracking-[0.08em]");
    expect(runtimeStatusBadgeLabelClassName).not.toContain("shadow");
    expect(runtimeStatusBadgeTableClassName).toContain("w-[72px]");
  });
});
