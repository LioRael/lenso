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
    <div className="isolate relative h-full min-w-0 overflow-hidden bg-[#080808]">
      <div className="pointer-events-none absolute top-0 right-0 left-0 z-[2] flex items-center justify-between px-4 py-2.5">
        <span className="pointer-events-auto font-sans text-[10px] font-medium uppercase tracking-[0.12em] text-[#5b5b5b]">
          Span Graph
        </span>
        <button
          className="pointer-events-auto flex items-center gap-1.5 font-mono text-[10px] text-[#5b5b5b] transition hover:text-[#f4f4f4]"
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
                  stroke="#333333"
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
                  "absolute h-[72px] w-[240px] rounded-[4px] border bg-[#0e0e0e] text-left transition hover:bg-[#141414]",
                  isSelected &&
                    "border-[#f3f724] shadow-[0_0_12px_rgba(243,247,36,0.2)] ring-1 ring-[#f3f724]/30",
                  !isSelected && isError && "border-[#ef4444]/45",
                  !isSelected &&
                    !isError &&
                    "border-[#1d1d1d] hover:border-[#3d3d3d]"
                )}
                key={span.id}
                onClick={() => onSelectSpan(span)}
                style={{ left: x, top: y }}
              >
                <span
                  className="absolute top-0 left-0 right-0 h-[3px] rounded-t-[4px]"
                  style={{ backgroundColor: color }}
                />
                <span className="flex h-full flex-col justify-between px-3 pt-2.5 pb-2">
                  <span className="flex items-start justify-between gap-2">
                    <span
                      className="rounded-[2px] border px-1.5 py-0.5 font-mono text-[8px] font-bold uppercase tracking-wider"
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
                        "rounded-[2px] px-1 py-0.5 font-mono text-[8px] uppercase tracking-wider",
                        isError
                          ? "bg-[#ef4444]/10 text-[#ef4444]"
                          : "bg-[#f3f724]/10 text-[#f3f724]"
                      )}
                    >
                      {span.kind}
                    </span>
                  </span>
                  <span className="min-w-0">
                    <span className="block truncate font-mono text-[11px] text-[#f4f4f4]">
                      {span.name}
                    </span>
                    <span className="block font-mono text-[9px] text-[#5b5b5b]">
                      {formatTraceDuration(span.durationMs)}
                    </span>
                  </span>
                </span>
                {isError ? (
                  <span className="absolute -top-1 -right-1 size-2.5 rounded-full border border-[#0e0e0e] bg-[#ef4444]" />
                ) : null}
              </button>
            );
          })}
        </div>
      </div>

      <div className="absolute bottom-10 left-4 z-[2] flex flex-col gap-1">
        {[Plus, Minus, Maximize2].map((Icon, index) => (
          <button
            aria-label="Flow view control"
            className="grid size-7 place-items-center rounded-[2px] border border-[#1d1d1d] bg-[#111111] text-[#9ca3af] transition hover:border-[#3d3d3d] hover:text-[#f4f4f4]"
            key={index}
            type="button"
          >
            <Icon size={14} />
          </button>
        ))}
      </div>

      <div className="absolute right-4 bottom-10 z-[2] h-[100px] w-[140px] overflow-hidden rounded-[2px] border border-[#1d1d1d] bg-black/90">
        <div className="scale-[0.13] origin-top-left p-4">
          {nodes.map(({ span, x, y }) => (
            <div
              className="absolute h-[72px] w-[240px] rounded-[4px]"
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

      <div className="absolute bottom-2 left-1/2 z-[2] flex -translate-x-1/2 items-center gap-4 rounded-[2px] border border-[#1d1d1d] bg-black/80 px-3 py-1.5 font-mono text-[9px] text-[#5b5b5b]">
        <span>Select nodes</span>
        <span>Wheel zoom</span>
        <span>Pan canvas</span>
      </div>
    </div>
  );
}
