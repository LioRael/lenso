import { Maximize2, Minus, Plus } from "lucide-react";

import type { TraceRun, TraceSpan } from "../../data/mock-runtime";
import { cn } from "../../lib/cn";
import {
  formatTraceDuration,
  serviceColor,
  spanDepth,
} from "../../lib/trace-style";

export function FlowView({
  selectedSpanId,
  trace,
  onSelectSpan,
}: {
  trace: TraceRun;
  selectedSpanId: string | null;
  onSelectSpan: (span: TraceSpan) => void;
}) {
  const nodes = trace.spans.map((span, index) => ({
    span,
    x: spanDepth(span, trace.spans) * 280,
    y: index * 92,
  }));

  return (
    <div className="isolate relative h-full min-w-0 overflow-hidden bg-(--sidebar)">
      <div className="pointer-events-none absolute top-0 right-0 left-0 z-2 flex items-center justify-between px-4 py-2.5">
        <span className="pointer-events-auto font-sans text-[11px] font-semibold uppercase tracking-[0.08em] text-(--muted)">
          Execution Graph
        </span>
        <button
          className="pointer-events-auto flex items-center gap-1.5 font-mono text-[11px] text-(--muted) transition hover:text-(--foreground)"
          type="button"
        >
          <Maximize2 size={12} />
          Frame
        </button>
      </div>

      <div className="relative z-0 h-full overflow-auto p-16">
        <div
          className="relative"
          style={{
            height: Math.max(420, nodes.length * 92),
            width: 980,
          }}
        >
          <svg
            aria-label="Trace flow connectors"
            className="pointer-events-none absolute inset-0 size-full"
          >
            <title>Trace flow connectors</title>
            {nodes.slice(0, -1).map((node, index) => {
              const next = nodes[index + 1];
              if (!next) {
                return null;
              }
              const fromX = node.x + 240;
              const fromY = node.y + 36;
              const toX = next.x;
              const toY = next.y + 36;
              const midX = (fromX + toX) / 2;
              return (
                <path
                  d={`M ${fromX} ${fromY} C ${midX} ${fromY}, ${midX} ${toY}, ${toX} ${toY}`}
                  fill="none"
                  key={`${node.span.id}-${next.span.id}`}
                  opacity="0.72"
                  stroke="var(--muted-deep)"
                  strokeDasharray="6 4"
                  strokeWidth="1.5"
                />
              );
            })}
          </svg>

          {nodes.map(({ span, x, y }) => {
            const color = serviceColor(span.service);
            const isSelected = selectedSpanId === span.id;
            const isError = span.status === "failed" || span.status === "dead";
            return (
              <button
                className={cn(
                  "absolute h-18 w-60 rounded-sm border bg-(--elevated) text-left transition hover:bg-(--hover)",
                  isSelected &&
                    "border-(--accent) shadow-[0_0_12px_color-mix(in_srgb,var(--accent)_22%,transparent)] ring-1 ring-[color-mix(in_srgb,var(--accent)_30%,transparent)]",
                  !isSelected &&
                    isError &&
                    "border-[color-mix(in_srgb,var(--error)_45%,transparent)]",
                  !isSelected &&
                    !isError &&
                    "border-(--border-subtle) hover:border-(--muted-deep)",
                )}
                key={span.id}
                onClick={() => onSelectSpan(span)}
                style={{ left: x, top: y }}
              >
                <span
                  className="absolute top-0 left-0 right-0 h-0.75 rounded-t-sm"
                  style={{ backgroundColor: color }}
                />
                <span className="flex h-full flex-col justify-between px-3 pt-2.5 pb-2">
                  <span className="flex items-start justify-between gap-2">
                    <span
                      className="rounded-xs border px-1.5 py-0.5 font-mono text-[10px] font-bold uppercase tracking-[0.06em]"
                      style={{
                        backgroundColor: `${color}18`,
                        borderColor: `${color}30`,
                        color,
                      }}
                    >
                      {span.service}
                    </span>
                    <span
                      className={cn(
                        "rounded-xs px-1.5 py-0.5 font-mono text-[10px] uppercase tracking-[0.06em]",
                        isError
                          ? "bg-[color-mix(in_srgb,var(--error)_10%,transparent)] text-(--error)"
                          : "bg-[color-mix(in_srgb,var(--accent)_10%,transparent)] text-(--accent)",
                      )}
                    >
                      {span.kind}
                    </span>
                  </span>
                  <span className="min-w-0">
                    <span className="block truncate font-mono text-[13px] text-(--foreground)">
                      {span.name}
                    </span>
                    <span className="block font-mono text-[11px] text-(--muted)">
                      {formatTraceDuration(span.durationMs)}
                    </span>
                  </span>
                </span>
                {isError ? (
                  <span className="absolute -top-1 -right-1 size-2.5 rounded-full border border-(--elevated) bg-[#ef4444]" />
                ) : null}
              </button>
            );
          })}
        </div>
      </div>

      <div className="absolute bottom-10 left-4 z-2 flex flex-col gap-1">
        {[Plus, Minus, Maximize2].map((Icon, index) => (
          <button
            aria-label="Flow view control"
            className="grid size-7 place-items-center rounded-xs border border-(--border-subtle) bg-(--elevated) text-(--secondary) transition hover:border-(--muted-deep) hover:text-(--foreground)"
            key={index}
            type="button"
          >
            <Icon size={14} />
          </button>
        ))}
      </div>

      <div className="absolute right-4 bottom-10 z-2 h-25 w-35 overflow-hidden rounded-xs border border-(--border-subtle) bg-[color-mix(in_srgb,var(--background)_90%,transparent)]">
        <div className="scale-13 origin-top-left p-4">
          {nodes.map(({ span, x, y }) => (
            <div
              className="absolute h-18 w-60 rounded-sm"
              key={span.id}
              style={{
                backgroundColor: serviceColor(span.service),
                left: x,
                opacity: selectedSpanId === span.id ? 1 : 0.45,
                top: y,
              }}
            />
          ))}
        </div>
      </div>

      <div className="absolute bottom-2 left-1/2 z-2 flex -translate-x-1/2 items-center gap-4 rounded-xs border border-(--border-subtle) bg-[color-mix(in_srgb,var(--background)_84%,transparent)] px-3 py-1.5 font-mono text-[11px] text-(--muted)">
        <span>Select nodes</span>
        <span>Wheel zoom</span>
        <span>Pan canvas</span>
      </div>
    </div>
  );
}
