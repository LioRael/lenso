import type { TraceRun, TraceSpan } from "../../data/mock-runtime";
import { cn } from "../../lib/cn";
import {
  formatTraceDuration,
  serviceColor,
  spanDepth,
  statusColor,
  traceTimelineEnd,
} from "../../lib/trace-style";

export function WaterfallView({
  selectedSpanId,
  trace,
  onSelectSpan,
}: {
  trace: TraceRun;
  selectedSpanId: string | null;
  onSelectSpan: (span: TraceSpan) => void;
}) {
  const timelineEnd = traceTimelineEnd(trace);

  return (
    <div className="isolate flex h-full min-w-0 flex-col overflow-hidden bg-[var(--background)]">
      <div className="flex items-center justify-between border-b border-[var(--border-subtle)] bg-[color-mix(in_srgb,var(--elevated)_36%,transparent)] px-3 py-2">
        <div className="flex items-center gap-2">
          <span className="font-sans text-[10px] font-semibold uppercase tracking-[0.12em] text-[var(--muted)]">
            Waterfall
          </span>
          <span className="font-mono text-[10px] text-[var(--muted)]">
            {trace.spans.length} of {trace.spans.length} spans
          </span>
        </div>
        <div className="font-mono text-[10px] text-[var(--muted)]">
          total {formatTraceDuration(timelineEnd)}
        </div>
      </div>
      <div className="grid min-w-0 grid-cols-[minmax(220px,300px)_minmax(0,1fr)] gap-4 border-b border-[var(--border-subtle)] bg-[color-mix(in_srgb,var(--elevated)_52%,transparent)] px-3 py-2 font-sans text-[10px] font-semibold uppercase tracking-[0.1em] text-[var(--muted)]">
        <span>Span</span>
        <div className="grid min-w-0 grid-cols-5 overflow-hidden">
          {[0, 25, 50, 75, 100].map((tick) => (
            <span className="font-mono" key={tick}>
              {formatTraceDuration((timelineEnd * tick) / 100)}
            </span>
          ))}
        </div>
      </div>
      <div className="min-h-0 flex-1 overflow-auto">
        {trace.spans.map((span) => {
          const left = clampPercent((span.startMs / timelineEnd) * 100);
          const rawWidth = (span.durationMs / timelineEnd) * 100;
          const width = Math.min(Math.max(rawWidth, 0.8), 100 - left);
          const depth = spanDepth(span, trace.spans);
          return (
            <button
              aria-label={`Select span ${span.name}`}
              className={cn(
                "grid w-full min-w-0 grid-cols-[minmax(220px,300px)_minmax(0,1fr)] items-center gap-4 px-3 py-1 text-left transition hover:bg-[color-mix(in_srgb,var(--hover)_64%,transparent)]",
                selectedSpanId === span.id &&
                  "bg-[var(--accent-soft)] shadow-[inset_2px_0_0_var(--accent)]"
              )}
              key={span.id}
              onClick={() => onSelectSpan(span)}
            >
              <span className="flex min-w-0 items-center gap-1.5 overflow-hidden">
                <span
                  className="h-6 flex-shrink-0 border-l border-[color-mix(in_srgb,var(--border-subtle)_64%,transparent)]"
                  style={{ marginLeft: depth * 14, width: depth > 0 ? 8 : 0 }}
                />
                <span
                  className="size-2 flex-shrink-0 rounded-[2px]"
                  style={{ backgroundColor: statusColor(span.status) }}
                />
                <span
                  className="rounded-[2px] border px-1.5 py-0.5 font-mono text-[10px] leading-none"
                  style={{
                    backgroundColor: `${serviceColor(span.service)}12`,
                    borderColor: `${serviceColor(span.service)}24`,
                    color: serviceColor(span.service),
                  }}
                >
                  {span.service}
                </span>
                <span className="truncate font-mono text-[12px] text-[var(--foreground)]">
                  {span.name}
                </span>
                <span className="ml-auto font-mono text-[11px] text-[var(--muted)]">
                  {formatTraceDuration(span.durationMs)}
                </span>
              </span>
              <span className="relative isolate h-6 min-w-0 overflow-hidden rounded-[2px] bg-[linear-gradient(90deg,transparent_0%,transparent_24.8%,var(--border-subtle)_25%,transparent_25.2%,transparent_49.8%,var(--border-subtle)_50%,transparent_50.2%,transparent_74.8%,var(--border-subtle)_75%,transparent_75.2%)]">
                <span
                  className="absolute top-1 h-4 min-w-[3px] rounded-[2px] transition-transform"
                  style={{
                    backgroundColor:
                      span.status === "failed" || span.status === "dead"
                        ? "#ef4444"
                        : serviceColor(span.service),
                    left: `${left}%`,
                    opacity: selectedSpanId === span.id ? 1 : 0.82,
                    transform:
                      selectedSpanId === span.id ? "scaleY(1.25)" : undefined,
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

function clampPercent(value: number) {
  return Math.min(100, Math.max(0, value));
}
