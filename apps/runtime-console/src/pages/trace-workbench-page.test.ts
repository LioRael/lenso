import { describe, expect, test } from "vitest";

import type { TraceHeader } from "../components/runtime/trace-header";
import { traceRuns } from "../data/mock-runtime";
import { traceWorkbenchDefaultViewMode } from "./trace-workbench-page";

describe("trace workbench page contracts", () => {
  test("defaults to the waterfall visualization mode", () => {
    const defaultViewMode: "waterfall" = traceWorkbenchDefaultViewMode;

    expect(defaultViewMode).toBe("waterfall");
  });

  test("keeps trace header props aligned with trace data", () => {
    const trace = traceRuns[0]!;
    const traceHeaderProps: Parameters<typeof TraceHeader>[0] = {
      onClose: () => undefined,
      onSelectSpan: () => undefined,
      trace,
    };

    expect(traceHeaderProps.trace.id).toBe(trace.id);
  });
});
