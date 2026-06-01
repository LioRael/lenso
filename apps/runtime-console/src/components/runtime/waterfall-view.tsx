import type { RuntimeStory, ExecutionNode } from "../../data/mock-runtime";
import { cn } from "../../lib/cn";
import {
  formatRuntimeDuration,
  serviceColor,
  nodeDepth,
  statusColor,
  runtimeTimelineEnd,
} from "../../lib/runtime-style";
import { runtimeWaterfallTableHeaderClassName } from "./runtime-table-header";
import { RuntimeViewHeader } from "./runtime-view-header";

export function WaterfallView({
  selectedNodeId,
  story,
  onSelectNode,
}: {
  story: RuntimeStory;
  selectedNodeId: string | null;
  onSelectNode: (node: ExecutionNode) => void;
}) {
  const timelineEnd = runtimeTimelineEnd(story);

  return (
    <div className="isolate flex h-full min-w-0 flex-col overflow-hidden bg-(--background)">
      <RuntimeViewHeader
        meta={`total ${formatRuntimeDuration(timelineEnd)}`}
        summary={`node detail · ${story.nodes.length} of ${story.nodes.length} nodes`}
        title="Waterfall"
      />
      <div className={runtimeWaterfallTableHeaderClassName}>
        <span>Node</span>
        <div className="grid min-w-0 grid-cols-5 overflow-hidden">
          {[0, 25, 50, 75, 100].map((tick) => (
            <span className="font-mono" key={tick}>
              {formatRuntimeDuration((timelineEnd * tick) / 100)}
            </span>
          ))}
        </div>
      </div>
      <div className="min-h-0 flex-1 overflow-auto">
        {story.nodes.map((node) => {
          const left = clampPercent((node.startMs / timelineEnd) * 100);
          const rawWidth = (node.durationMs / timelineEnd) * 100;
          const width = Math.min(Math.max(rawWidth, 0.8), 100 - left);
          const depth = nodeDepth(node, story.nodes);
          return (
            <button
              aria-label={`Select node ${node.name}`}
              className={cn(
                "grid w-full min-w-0 grid-cols-[minmax(260px,340px)_minmax(0,1fr)] items-center gap-4 px-3 py-1.5 text-left transition hover:bg-[color-mix(in_srgb,var(--hover)_64%,transparent)]",
                selectedNodeId === node.id &&
                  "bg-(--accent-soft) shadow-[inset_2px_0_0_var(--accent)]"
              )}
              key={node.id}
              onClick={() => onSelectNode(node)}
              type="button"
            >
              <span className="flex min-w-0 items-center gap-1.5 overflow-hidden">
                <span
                  className="h-6 shrink-0 border-l border-[color-mix(in_srgb,var(--border-subtle)_64%,transparent)]"
                  style={{ marginLeft: depth * 14, width: depth > 0 ? 8 : 0 }}
                />
                <span
                  className="size-2 shrink-0 rounded-xs"
                  style={{ backgroundColor: statusColor(node.status) }}
                />
                <span
                  className="max-w-26 shrink-0 truncate whitespace-nowrap rounded-xs border px-1.5 py-0.5 font-mono text-[11px] leading-3.5"
                  style={{
                    backgroundColor: `${serviceColor(node.service)}12`,
                    borderColor: `${serviceColor(node.service)}24`,
                    color: serviceColor(node.service),
                  }}
                >
                  {node.service}
                </span>
                <span className="truncate font-mono text-[13px] text-(--foreground)">
                  {node.name}
                </span>
                <span className="ml-auto font-mono text-xs text-(--muted)">
                  {formatRuntimeDuration(node.durationMs)}
                </span>
              </span>
              <span className="relative isolate h-6 min-w-0 overflow-hidden rounded-xs bg-[linear-gradient(90deg,transparent_0%,transparent_24.8%,var(--border-subtle)_25%,transparent_25.2%,transparent_49.8%,var(--border-subtle)_50%,transparent_50.2%,transparent_74.8%,var(--border-subtle)_75%,transparent_75.2%)]">
                <span
                  className="absolute top-1 h-4 min-w-0.75 rounded-xs transition-transform"
                  style={{
                    backgroundColor:
                      node.status === "failed" || node.status === "dead"
                        ? "#ef4444"
                        : serviceColor(node.service),
                    left: `${left}%`,
                    opacity: selectedNodeId === node.id ? 1 : 0.82,
                    transform:
                      selectedNodeId === node.id ? "scaleY(1.25)" : undefined,
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
