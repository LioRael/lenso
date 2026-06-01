import { Cloud, Mail, Route, ServerCog, Workflow } from "lucide-react";
import type { ComponentType } from "react";

import type { TraceRun, TraceSpan } from "../../data/mock-runtime";
import { cn } from "../../lib/cn";
import { buildRuntimeStory, type StoryNodeType } from "../../lib/story";
import {
  formatTraceDuration,
  serviceColor,
  statusColor,
  traceTimelineEnd,
} from "../../lib/trace-style";

export function StoryTimelineView({
  selectedSpanId,
  trace,
  onSelectSpan,
}: {
  trace: TraceRun;
  selectedSpanId: string | null;
  onSelectSpan: (span: TraceSpan) => void;
}) {
  const story = buildRuntimeStory(trace);
  const timelineEnd = traceTimelineEnd(trace);

  return (
    <div className="grid h-full min-h-0 min-w-0 grid-rows-[auto_minmax(0,1fr)] overflow-hidden bg-(--background)">
      <div className="flex min-w-0 items-center justify-between gap-3 overflow-hidden border-b border-(--border-subtle) bg-[color-mix(in_srgb,var(--elevated)_40%,transparent)] px-3 py-2">
        <div className="flex min-w-0 items-center gap-2 overflow-hidden">
          <span className="font-sans text-[11px] font-semibold uppercase tracking-[0.08em] text-(--muted)">
            Business Timeline
          </span>
          <span className="min-w-0 truncate font-mono text-[11px] text-(--muted)">
            {story.nodes.length} execution nodes from one correlation
          </span>
        </div>
        <div className="shrink-0 font-mono text-[11px] text-(--muted)">
          total {formatTraceDuration(timelineEnd)}
        </div>
      </div>

      <div className="min-h-0 overflow-auto px-4 py-4">
        <div className="mx-auto w-full max-w-5xl">
          <div className="mb-4 grid min-w-0 grid-cols-[minmax(180px,260px)_minmax(0,1fr)] gap-4 border-b border-(--border-subtle) pb-2 text-[11px] font-semibold uppercase tracking-[0.06em] text-(--muted) max-md:grid-cols-1">
            <span>Story Flow</span>
            <div className="grid min-w-0 grid-cols-5 overflow-hidden font-mono">
              {[0, 25, 50, 75, 100].map((tick) => (
                <span key={tick}>
                  {formatTraceDuration((timelineEnd * tick) / 100)}
                </span>
              ))}
            </div>
          </div>

          <div className="grid gap-3">
            {story.nodes.map((node, index) => {
              const Icon = nodeIcon[node.type];
              const tone = nodeTone[node.type];
              const left = clampPercent((node.startMs / timelineEnd) * 100);
              const width = Math.min(
                Math.max((node.durationMs / timelineEnd) * 100, 1.5),
                100 - left
              );
              const selected = selectedSpanId === node.span.id;
              const errored =
                node.status === "failed" || node.status === "dead";

              return (
                <button
                  aria-label={`Open ${node.typeLabel} ${node.title}`}
                  className={cn(
                    "group grid min-w-0 grid-cols-[minmax(180px,260px)_minmax(0,1fr)] gap-4 text-left transition max-md:grid-cols-1",
                    selected && "scale-[1.004]"
                  )}
                  key={node.id}
                  onClick={() => onSelectSpan(node.span)}
                  type="button"
                >
                  <span
                    className={cn(
                      "relative min-w-0 border bg-(--surface) px-3 py-2.5 shadow-[0_12px_28px_var(--shadow-soft)] transition group-hover:border-(--border)",
                      tone.card,
                      selected &&
                        "border-(--accent) shadow-[inset_2px_0_0_var(--accent)]"
                    )}
                  >
                    {index > 0 ? (
                      <span className="-top-3.5 absolute left-6 h-3.5 w-px bg-(--border)" />
                    ) : null}
                    <span className="flex min-w-0 items-start gap-2">
                      <span
                        className={cn(
                          "grid size-8 shrink-0 place-items-center border",
                          tone.icon
                        )}
                      >
                        <Icon size={15} strokeWidth={1.8} />
                      </span>
                      <span className="min-w-0 flex-1">
                        <span className="flex min-w-0 items-center gap-2">
                          <span className="truncate font-mono text-[10px] font-semibold uppercase tracking-[0.06em]">
                            {node.typeLabel}
                          </span>
                          <span
                            className="size-1.5 shrink-0 rounded-full"
                            style={{
                              backgroundColor: statusColor(node.status),
                            }}
                          />
                        </span>
                        <span className="mt-1 block truncate text-[13px] font-semibold text-(--foreground)">
                          {node.title}
                        </span>
                        <span className="mt-1 flex min-w-0 items-center gap-2 font-mono text-[10px] text-(--muted)">
                          <span
                            className="h-1.5 w-1.5 shrink-0"
                            style={{
                              backgroundColor: serviceColor(node.service),
                            }}
                          />
                          <span className="truncate">{node.service}</span>
                          <span className="ml-auto shrink-0">
                            {formatTraceDuration(node.durationMs)}
                          </span>
                        </span>
                      </span>
                    </span>
                    {node.error ? (
                      <span className="mt-2 block truncate border-l-2 border-[#ef4444] pl-2 font-mono text-[11px] text-[#ff8b86]">
                        {node.error}
                      </span>
                    ) : null}
                  </span>

                  <span className="grid min-h-18 min-w-0 items-center max-md:hidden">
                    <span className="relative h-9 min-w-0 overflow-hidden border border-(--border-subtle) bg-[linear-gradient(90deg,transparent_0%,transparent_24.8%,var(--border-subtle)_25%,transparent_25.2%,transparent_49.8%,var(--border-subtle)_50%,transparent_50.2%,transparent_74.8%,var(--border-subtle)_75%,transparent_75.2%)]">
                      <span
                        className={cn(
                          "absolute top-2 h-5 min-w-1 transition",
                          errored && "shadow-[0_0_16px_rgba(239,68,68,0.3)]"
                        )}
                        style={{
                          backgroundColor: errored
                            ? "#ef4444"
                            : serviceColor(node.service),
                          left: `${left}%`,
                          opacity: selected ? 1 : 0.82,
                          transform: selected ? "scaleY(1.22)" : undefined,
                          width: `${width}%`,
                        }}
                      />
                    </span>
                  </span>
                </button>
              );
            })}
          </div>
        </div>
      </div>
    </div>
  );
}

const nodeIcon: Record<
  StoryNodeType,
  ComponentType<{ size?: number; strokeWidth?: number }>
> = {
  event: Mail,
  external_provider: Cloud,
  function: Workflow,
  request: Route,
  worker: ServerCog,
};

const nodeTone: Record<StoryNodeType, { card: string; icon: string }> = {
  event: {
    card: "border-sky-300/20 text-sky-200",
    icon: "border-sky-300/30 bg-sky-300/10 text-sky-200",
  },
  external_provider: {
    card: "border-rose-300/24 text-rose-200",
    icon: "border-rose-300/35 bg-rose-300/10 text-rose-200",
  },
  function: {
    card: "border-emerald-300/20 text-emerald-200",
    icon: "border-emerald-300/30 bg-emerald-300/10 text-emerald-200",
  },
  request: {
    card: "border-[color-mix(in_srgb,var(--accent)_26%,transparent)] text-(--accent)",
    icon: "border-[color-mix(in_srgb,var(--accent)_34%,transparent)] bg-(--accent-soft) text-(--accent)",
  },
  worker: {
    card: "border-amber-300/20 text-amber-200",
    icon: "border-amber-300/30 bg-amber-300/10 text-amber-200",
  },
};

function clampPercent(value: number) {
  return Math.min(100, Math.max(0, value));
}
