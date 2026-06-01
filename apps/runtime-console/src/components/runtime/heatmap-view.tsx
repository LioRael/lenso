import type { TraceRun, TraceSpan } from "../../data/mock-runtime";
import { cn } from "../../lib/cn";
import { formatTraceDuration, traceTimelineEnd } from "../../lib/trace-style";

export function HeatmapView({
  selectedSpanId,
  trace,
  onSelectSpan,
}: {
  trace: TraceRun;
  selectedSpanId: string | null;
  onSelectSpan: (span: TraceSpan) => void;
}) {
  const timelineEnd = traceTimelineEnd(trace);
  const cells = Array.from({ length: 120 }, (_, index) => {
    const bucketStart = (index / 120) * timelineEnd;
    const span = trace.spans.find(
      (item) =>
        bucketStart >= item.startMs &&
        bucketStart <= item.startMs + item.durationMs
    );
    return { bucketStart, index, span };
  });

  return (
    <div className="isolate flex h-full min-w-0 flex-col overflow-hidden bg-black">
      <div className="flex items-center justify-between border-b border-[#1d1d1d] bg-[#0a0a0a] px-3 py-2">
        <div className="flex items-center gap-2">
          <span className="font-sans text-[10px] font-semibold uppercase tracking-[0.12em] text-[#f3f724]">
            Heatmap
          </span>
          <span className="font-mono text-[10px] text-[#5b5b5b]">
            {cells.length} buckets across {formatTraceDuration(timelineEnd)}
          </span>
        </div>
        <div className="flex items-center gap-2 font-mono text-[9px] text-[#5b5b5b]">
          <span>idle</span>
          <span className="text-[#3b82f6]">short</span>
          <span className="text-[#22c55e]">work</span>
          <span className="text-[#f3f724]">slow</span>
          <span className="text-[#ef4444]">fault</span>
        </div>
      </div>
      <div className="min-h-0 flex-1 overflow-auto bg-[#050505] p-3">
        <div className="grid grid-cols-[repeat(20,minmax(0,1fr))] gap-0.5">
          {cells.map((cell) => (
            <button
              aria-label={
                cell.span
                  ? `Select span ${cell.span.name}`
                  : `Select empty heatmap bucket ${Math.round(cell.bucketStart)}ms`
              }
              className={cn(
                "relative aspect-[1.25] rounded-[1px] border border-[#1d1d1d] bg-[#111111] transition hover:z-[1] hover:border-[#9ca3af]",
                cell.span && heatTone(cell.span),
                selectedSpanId === cell.span?.id &&
                  "border-[#f3f724] outline outline-1 outline-[#f3f724]"
              )}
              key={cell.index}
              onClick={() => cell.span && onSelectSpan(cell.span)}
              title={cell.span?.name ?? formatTraceDuration(cell.bucketStart)}
            />
          ))}
        </div>
      </div>
    </div>
  );
}

function heatTone(span: TraceSpan) {
  if (span.status === "failed" || span.status === "dead") {
    return "bg-[#ef4444]/85";
  }
  if (span.durationMs > 1000) {
    return "bg-[#f3f724]/75";
  }
  if (span.durationMs > 200) {
    return "bg-[#22c55e]/55";
  }
  return "bg-[#3b82f6]/35";
}
