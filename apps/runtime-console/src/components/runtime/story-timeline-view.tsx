import { Cloud, Mail, Route, ServerCog, Workflow } from "lucide-react";
import type { ComponentType } from "react";

import type { RuntimeStory, ExecutionNode } from "../../data/mock-runtime";
import { cn } from "../../lib/cn";
import {
  formatRuntimeDuration,
  serviceColor,
  statusColor,
  runtimeTimelineEnd,
} from "../../lib/runtime-style";
import { buildRuntimeStory, type RuntimeNodeType } from "../../lib/story";
import { runtimeTimelineTableHeaderClassName } from "./runtime-table-header";
import { RuntimeViewHeader } from "./runtime-view-header";

export function StoryTimelineView({
  selectedNodeId,
  story,
  onSelectNode,
}: {
  story: RuntimeStory;
  selectedNodeId: string | null;
  onSelectNode: (node: ExecutionNode) => void;
}) {
  const storySummary = buildRuntimeStory(story);
  const timelineEnd = runtimeTimelineEnd(story);

  return (
    <div className="isolate flex h-full min-w-0 flex-col overflow-hidden bg-(--background)">
      <RuntimeViewHeader
        meta={`total ${formatRuntimeDuration(timelineEnd)}`}
        summary={`${storySummary.nodeCount} execution nodes from one correlation`}
        title="Business Timeline"
      />

      <div className={runtimeTimelineTableHeaderClassName}>
        <span>Story Flow</span>
        <div className="grid min-w-0 grid-cols-5 overflow-hidden font-mono">
          {[0, 25, 50, 75, 100].map((tick) => (
            <span key={tick}>
              {formatRuntimeDuration((timelineEnd * tick) / 100)}
            </span>
          ))}
        </div>
      </div>

      <div className="min-h-0 flex-1 overflow-auto px-4 py-4">
        <div className="mx-auto w-full max-w-5xl">
          <div className="grid gap-3">
            {storySummary.nodes.map((node, index) => {
              const Icon = nodeIcon[node.type];
              const tone = nodeTone[node.type];
              const left = clampPercent((node.timestamp / timelineEnd) * 100);
              const width = Math.min(
                Math.max((node.duration / timelineEnd) * 100, 1.5),
                100 - left
              );
              const selected = selectedNodeId === node.node.id;
              const errored =
                node.status === "failed" || node.status === "dead";

              return (
                <button
                  aria-label={`Open ${node.typeLabel} ${node.name}`}
                  className={cn(
                    "group grid min-w-0 grid-cols-[minmax(180px,260px)_minmax(0,1fr)] gap-4 text-left transition max-md:grid-cols-1",
                    selected && "scale-[1.004]"
                  )}
                  key={node.id}
                  onClick={() => onSelectNode(node.node)}
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
                          {node.name}
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
                            {formatRuntimeDuration(node.duration)}
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
  RuntimeNodeType,
  ComponentType<{ size?: number; strokeWidth?: number }>
> = {
  event: Mail,
  external: Cloud,
  function: Workflow,
  request: Route,
  worker: ServerCog,
};

const nodeTone: Record<RuntimeNodeType, { card: string; icon: string }> = {
  event: {
    card: "border-sky-300/20 text-sky-200",
    icon: "border-sky-300/30 bg-sky-300/10 text-sky-200",
  },
  external: {
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
