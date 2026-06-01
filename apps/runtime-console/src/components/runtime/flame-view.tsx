import type { TraceRun, TraceSpan } from "../../data/mock-runtime";
import { cn } from "../../lib/cn";
import {
  formatTraceDuration,
  serviceColor,
  traceTimelineEnd,
} from "../../lib/trace-style";

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
  const timelineEnd = traceTimelineEnd(trace);
  return (
    <div className="isolate flex h-full min-w-0 flex-col overflow-hidden bg-[var(--background)]">
      <div className="flex items-center justify-between border-b border-[var(--border-subtle)] bg-[var(--surface)] px-4 py-2">
        <div className="flex items-center gap-2">
          <span className="font-sans text-[10px] uppercase tracking-[0.12em] text-[var(--muted)]">
            Color
          </span>
          <span className="rounded-[2px] bg-[var(--accent)] px-2 py-0.5 font-mono text-[10px] font-semibold text-[var(--inverse)]">
            service
          </span>
          <span className="rounded-[2px] border border-[var(--border-subtle)] bg-[var(--elevated)] px-2 py-0.5 font-mono text-[10px] text-[var(--muted)]">
            status
          </span>
        </div>
        <span className="font-mono text-[10px] text-[var(--muted)]">
          {formatTraceDuration(timelineEnd)}
        </span>
      </div>
      <div className="min-h-0 flex-1 overflow-auto p-4">
        {levels.map((level, index) => (
          <div
            className="relative isolate h-8 overflow-hidden border-b border-[color-mix(in_srgb,var(--border-subtle)_60%,transparent)]"
            key={index}
          >
            {level.map((span) => {
              const left = clampPercent((span.startMs / timelineEnd) * 100);
              const rawWidth = (span.durationMs / timelineEnd) * 100;
              const width = Math.min(Math.max(rawWidth, 3), 100 - left);
              return (
                <button
                  className={cn(
                    "absolute top-1 h-6 overflow-hidden rounded-[2px] border px-2 text-left font-mono text-[10px] text-[var(--foreground)] transition hover:brightness-125",
                    selectedSpanId === span.id &&
                      "shadow-[0_0_0_1px_var(--accent),0_0_8px_color-mix(in_srgb,var(--accent)_25%,transparent)]"
                  )}
                  key={span.id}
                  onClick={() => onSelectSpan(span)}
                  style={{
                    backgroundColor:
                      span.status === "failed" || span.status === "dead"
                        ? "#ef4444"
                        : `${serviceColor(span.service)}cc`,
                    borderColor:
                      span.status === "failed" || span.status === "dead"
                        ? "#ef4444"
                        : `${serviceColor(span.service)}99`,
                    left: `${left}%`,
                    width: `${width}%`,
                  }}
                >
                  <span className="truncate">
                    {span.name} · {formatTraceDuration(span.durationMs)}
                  </span>
                </button>
              );
            })}
          </div>
        ))}
      </div>
    </div>
  );
}

function clampPercent(value: number) {
  return Math.min(100, Math.max(0, value));
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
