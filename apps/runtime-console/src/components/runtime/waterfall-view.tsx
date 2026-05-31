import type { TraceRun, TraceSpan } from "../../data/mock-runtime";
import { cn } from "../../lib/cn";

export function WaterfallView({
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
      <div className="mb-1.5 grid grid-cols-[220px_minmax(0,1fr)] gap-3 font-mono text-[9px] uppercase tracking-[0.04em] text-slate-700">
        <span>span</span>
        <div className="grid grid-cols-5 border-l border-white/10 pl-2">
          {[0, 25, 50, 75, 100].map((tick) => (
            <span key={tick}>{tick}%</span>
          ))}
        </div>
      </div>
      <div className="grid gap-1">
        {trace.spans.map((span) => {
          const left = (span.startMs / trace.durationMs) * 100;
          const width = Math.max(
            (span.durationMs / trace.durationMs) * 100,
            1.2
          );
          return (
            <button
              aria-label={`Select span ${span.name}`}
              className={cn(
                "grid grid-cols-[220px_minmax(0,1fr)] gap-3 border border-transparent bg-transparent py-1 text-left font-mono text-[11px] text-slate-500 hover:border-white/10 hover:bg-white/[0.025]",
                selectedSpanId === span.id &&
                  "border-cyan-300/25 bg-cyan-300/[0.055]"
              )}
              key={span.id}
              onClick={() => onSelectSpan(span)}
            >
              <span className="grid min-w-0 grid-cols-[6px_minmax(0,1fr)_42px] items-center gap-2 px-1">
                <span
                  className={cn("size-1.5 rounded-full", dotTone(span.status))}
                />
                <span className="truncate text-slate-300">{span.name}</span>
                <span className="text-right text-slate-600">
                  {span.durationMs}ms
                </span>
              </span>
              <span className="relative h-5 border-l border-white/10 bg-[linear-gradient(90deg,rgba(255,255,255,0.026)_1px,transparent_1px)] bg-[length:20%_100%]">
                <span
                  className={cn(
                    "absolute top-1 h-3 border shadow-[0_0_16px_rgba(0,0,0,0.35)]",
                    spanTone(span.status)
                  )}
                  style={{
                    left: `${left}%`,
                    width: `${width}%`,
                  }}
                />
              </span>
            </button>
          );
        })}
      </div>
    </div>
  );
}

function spanTone(status: TraceSpan["status"]) {
  if (status === "failed" || status === "dead") {
    return "border-rose-300/60 bg-rose-400/45";
  }
  if (status === "running" || status === "pending" || status === "processing") {
    return "border-amber-300/50 bg-amber-300/25";
  }
  return "border-cyan-300/35 bg-cyan-300/22";
}

function dotTone(status: TraceSpan["status"]) {
  if (status === "failed" || status === "dead") {
    return "bg-rose-400";
  }
  if (status === "running" || status === "pending" || status === "processing") {
    return "bg-amber-300";
  }
  return "bg-cyan-300";
}
