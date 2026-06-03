import type { RuntimeStory, ExecutionNode } from "../../data/mock-runtime";
import { cn } from "../../lib/cn";
import {
  formatRuntimeDuration,
  serviceColor,
  runtimeTimelineEnd,
  timelineSegmentLayout,
} from "../../lib/runtime-style";
import { buildFlameLevels } from "./flame-model";
import { RuntimeViewHeader } from "./runtime-view-header";

export function FlameView({
  selectedNodeId,
  story,
  onSelectNode,
}: {
  story: RuntimeStory;
  selectedNodeId: string | null;
  onSelectNode: (node: ExecutionNode) => void;
}) {
  const levels = buildFlameLevels(story.nodes);
  const timelineEnd = runtimeTimelineEnd(story);
  return (
    <div className="isolate flex h-full min-w-0 flex-col overflow-hidden bg-(--background)">
      <RuntimeViewHeader
        meta={formatRuntimeDuration(timelineEnd)}
        summary="color by service and status"
        title="Flame"
      />
      <div className="min-h-0 flex-1 overflow-auto p-4">
        {levels.map((level) => (
          <div
            className="relative isolate h-9 overflow-hidden border-b border-[color-mix(in_srgb,var(--border-subtle)_60%,transparent)]"
            key={level.map((node) => node.id).join(":")}
          >
            {level.map((node) => {
              const segment = timelineSegmentLayout({
                durationMs: node.durationMs,
                minWidthPercent: 3,
                startMs: node.startMs,
                timelineEnd,
              });
              return (
                <button
                  className={cn(
                    "absolute top-1 h-7 overflow-hidden rounded-xs border px-2 text-left font-mono text-[12px] text-(--foreground) transition hover:brightness-125",
                    selectedNodeId === node.id &&
                      "shadow-[0_0_0_1px_var(--accent),0_0_8px_color-mix(in_srgb,var(--accent)_25%,transparent)]"
                  )}
                  key={node.id}
                  onClick={() => onSelectNode(node)}
                  style={{
                    backgroundColor:
                      node.status === "failed" || node.status === "dead"
                        ? "#ef4444"
                        : `${serviceColor(node.service)}cc`,
                    borderColor:
                      node.status === "failed" || node.status === "dead"
                        ? "#ef4444"
                        : `${serviceColor(node.service)}99`,
                    left: `${segment.left}%`,
                    width: `${segment.width}%`,
                  }}
                  type="button"
                >
                  <span className="truncate">
                    {node.name} · {formatRuntimeDuration(node.durationMs)}
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
