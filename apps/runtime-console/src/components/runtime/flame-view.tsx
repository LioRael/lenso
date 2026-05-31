import type { TraceRun, TraceSpan } from "../../data/mock-runtime";
import { cn } from "../../lib/cn";

export function FlameView({
  selectedSpanId,
  trace,
  onSelectSpan,
}: {
  trace: TraceRun;
  selectedSpanId: string | null;
  onSelectSpan: (span: TraceSpan) => void;
}) {
  const levels = buildLevels(trace.spans);
  return (
    <div className="h-full overflow-auto p-2">
      <div className="grid gap-1">
        {levels.map((level, index) => (
          <div
            className="relative h-7 border-b border-white/[0.04]"
            key={index}
          >
            {level.map((span) => {
              const left = (span.startMs / trace.durationMs) * 100;
              const width = Math.max(
                (span.durationMs / trace.durationMs) * 100,
                3
              );
              return (
                <button
                  className={cn(
                    "absolute top-1 h-5 overflow-hidden border px-1.5 text-left font-mono text-[10px] text-slate-200 hover:brightness-125",
                    flameTone(span.status),
                    selectedSpanId === span.id &&
                      "outline outline-1 outline-cyan-200"
                  )}
                  key={span.id}
                  onClick={() => onSelectSpan(span)}
                  style={{
                    left: `${left}%`,
                    width: `${width}%`,
                  }}
                >
                  <span className="truncate">{span.name}</span>
                </button>
              );
            })}
          </div>
        ))}
      </div>
    </div>
  );
}

function buildLevels(spans: TraceSpan[]) {
  const byParent = new Map<string | undefined, TraceSpan[]>();
  spans.forEach((span) => {
    const children = byParent.get(span.parentId) ?? [];
    children.push(span);
    byParent.set(span.parentId, children);
  });

  const levels: TraceSpan[][] = [];
  const visit = (span: TraceSpan, depth: number) => {
    levels[depth] = [...(levels[depth] ?? []), span];
    (byParent.get(span.id) ?? []).forEach((child) => visit(child, depth + 1));
  };

  (byParent.get(undefined) ?? []).forEach((span) => visit(span, 0));
  return levels;
}

function flameTone(status: TraceSpan["status"]) {
  if (status === "failed" || status === "dead") {
    return "border-rose-300/50 bg-rose-500/35";
  }
  if (status === "published" || status === "completed") {
    return "border-cyan-300/30 bg-cyan-400/18";
  }
  return "border-amber-300/40 bg-amber-300/20";
}
