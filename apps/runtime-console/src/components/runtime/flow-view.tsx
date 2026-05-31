import { ArrowRight } from "lucide-react";

import type { TraceRun, TraceSpan } from "../../data/mock-runtime";
import { cn } from "../../lib/cn";

export function FlowView({
  selectedSpanId,
  trace,
  onSelectSpan,
}: {
  trace: TraceRun;
  selectedSpanId: string | null;
  onSelectSpan: (span: TraceSpan) => void;
}) {
  return (
    <div className="h-full overflow-auto p-2">
      <div className="grid gap-1.5">
        {trace.spans.map((span, index) => (
          <div
            className="grid grid-cols-[minmax(0,1fr)_20px] items-center gap-2"
            key={span.id}
          >
            <button
              className={cn(
                "grid grid-cols-[8px_minmax(0,1fr)_auto_auto] items-center gap-2 border border-white/10 bg-white/[0.018] px-2 py-1.5 text-left font-mono text-[11px] text-slate-300 hover:bg-white/[0.045]",
                selectedSpanId === span.id &&
                  "border-cyan-300/35 bg-cyan-300/[0.065]"
              )}
              onClick={() => onSelectSpan(span)}
            >
              <span
                className={cn("size-1.5 rounded-full", dotTone(span.status))}
              />
              <span className="truncate">{span.name}</span>
              <span className="text-slate-600">{span.service}</span>
              <span className="text-slate-700">{span.durationMs}ms</span>
            </button>
            {index < trace.spans.length - 1 ? (
              <ArrowRight className="text-slate-700" size={14} />
            ) : null}
          </div>
        ))}
      </div>
    </div>
  );
}

function dotTone(status: TraceSpan["status"]) {
  if (status === "failed" || status === "dead") {
    return "bg-rose-400";
  }
  if (status === "pending" || status === "processing" || status === "running") {
    return "bg-amber-300";
  }
  return "bg-cyan-300";
}
