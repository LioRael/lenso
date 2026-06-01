import type { TraceHeader } from "../components/runtime/trace-header";
import { traceRuns } from "../data/mock-runtime";
import { traceWorkbenchDefaultViewMode } from "./trace-workbench-page";

const defaultViewMode: "waterfall" = traceWorkbenchDefaultViewMode;
void defaultViewMode;

const trace = traceRuns[0]!;
const traceHeaderProps: Parameters<typeof TraceHeader>[0] = {
  onClose: () => undefined,
  onSelectSpan: () => undefined,
  trace,
};

if (traceHeaderProps.trace.id !== trace.id) {
  throw new Error("trace header props should preserve the selected trace");
}
