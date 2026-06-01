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
    <div className="isolate flex h-full min-w-0 flex-col overflow-hidden bg-[var(--background)]">
      <div className="flex items-center justify-between border-b border-[var(--border-subtle)] bg-[var(--surface)] px-3 py-2">
        <div className="flex items-center gap-2">
          <span className="font-sans text-[11px] font-semibold uppercase tracking-[0.08em] text-[var(--accent)]">
            Heatmap
          </span>
          <span className="font-mono text-[11px] text-[var(--muted)]">
            {cells.length} buckets across {formatTraceDuration(timelineEnd)}
          </span>
        </div>
        <div className="flex items-center gap-2 font-mono text-[10px] text-[var(--muted)]">
          <span>idle</span>
          <span className="text-[#3b82f6]">short</span>
          <span className="text-[#22c55e]">work</span>
          <span className="text-[var(--accent)]">slow</span>
          <span className="text-[#ef4444]">fault</span>
        </div>
      </div>
      <div className="min-h-0 flex-1 overflow-auto bg-[var(--background)] p-3">
        <div className="grid grid-cols-[repeat(20,minmax(0,1fr))] gap-0.5">
          {cells.map((cell) => (
            <button
              aria-label={
                cell.span
                  ? `Select span ${cell.span.name}`
                  : `Select empty heatmap bucket ${Math.round(cell.bucketStart)}ms`
              }
              className={cn(
                "relative aspect-[1.25] rounded-[1px] border border-[var(--border-subtle)] bg-[var(--elevated)] transition hover:z-[1] hover:border-[var(--secondary)]",
                cell.span && heatTone(cell.span),
                selectedSpanId === cell.span?.id &&
                  "border-[var(--accent)] outline outline-1 outline-[var(--accent)]"
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
    return "bg-[color-mix(in_srgb,var(--accent)_75%,transparent)]";
  }
  if (span.durationMs > 200) {
    return "bg-[#22c55e]/55";
  }
  return "bg-[#3b82f6]/35";
}
