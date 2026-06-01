import type { TraceRun, TraceSpan } from "../../data/mock-runtime";
import { cn } from "../../lib/cn";
import {
  formatTraceDuration,
  serviceColor,
  traceTimelineEnd,
} from "../../lib/trace-style";
import { TraceViewHeader } from "./trace-view-header";

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
    <div className="isolate flex h-full min-w-0 flex-col overflow-hidden bg-(--background)">
      <TraceViewHeader
        meta={formatTraceDuration(timelineEnd)}
        summary="color by service and status"
        title="Flame"
      />
      <div className="min-h-0 flex-1 overflow-auto p-4">
        {levels.map((level, index) => (
          <div
            className="relative isolate h-9 overflow-hidden border-b border-[color-mix(in_srgb,var(--border-subtle)_60%,transparent)]"
            key={index}
          >
            {level.map((span) => {
              const left = clampPercent((span.startMs / timelineEnd) * 100);
              const rawWidth = (span.durationMs / timelineEnd) * 100;
              const width = Math.min(Math.max(rawWidth, 3), 100 - left);
              return (
                <button
                  className={cn(
                    "absolute top-1 h-7 overflow-hidden rounded-xs border px-2 text-left font-mono text-[12px] text-(--foreground) transition hover:brightness-125",
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
                  type="button"
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
