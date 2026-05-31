import type { TraceRun, TraceSpan } from "../../data/mock-runtime";
import { cn } from "../../lib/cn";

export function HeatmapView({
  selectedSpanId,
  trace,
  onSelectSpan,
}: {
  trace: TraceRun;
  selectedSpanId: string | null;
  onSelectSpan: (span: TraceSpan) => void;
}) {
  const cells = Array.from({ length: 72 }, (_, index) => {
    const bucketStart = (index / 72) * trace.durationMs;
    const span = trace.spans.find(
      (item) =>
        bucketStart >= item.startMs &&
        bucketStart <= item.startMs + item.durationMs
    );
    return { bucketStart, index, span };
  });

  return (
    <div className="h-full overflow-auto p-2">
      <div className="grid grid-cols-12 gap-0.5">
        {cells.map((cell) => (
          <button
            aria-label={
              cell.span
                ? `Select span ${cell.span.name}`
                : `Select empty heatmap bucket ${Math.round(cell.bucketStart)}ms`
            }
            className={cn(
              "aspect-[1.8] border border-white/[0.045] bg-white/[0.018] hover:border-white/15",
              cell.span && heatTone(cell.span),
              selectedSpanId === cell.span?.id &&
                "outline outline-1 outline-cyan-200"
            )}
            key={cell.index}
            onClick={() => cell.span && onSelectSpan(cell.span)}
            title={cell.span?.name ?? `${Math.round(cell.bucketStart)}ms`}
          />
        ))}
      </div>
      <div className="mt-3 grid grid-cols-4 gap-2 font-mono text-[10px] text-slate-700">
        <span>low latency</span>
        <span className="text-cyan-300">active</span>
        <span className="text-amber-300">slow</span>
        <span className="text-rose-300">error</span>
      </div>
    </div>
  );
}

function heatTone(span: TraceSpan) {
  if (span.status === "failed" || span.status === "dead") {
    return "bg-rose-400/55";
  }
  if (span.durationMs > 1000) {
    return "bg-amber-300/45";
  }
  if (span.durationMs > 200) {
    return "bg-cyan-300/35";
  }
  return "bg-blue-300/20";
}
