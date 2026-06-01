import type { RuntimeStory, ExecutionNode } from "../../data/mock-runtime";
import { cn } from "../../lib/cn";
import {
  formatTraceDuration,
  serviceColor,
  traceTimelineEnd,
} from "../../lib/trace-style";
import { TraceViewHeader } from "./trace-view-header";

export function FlameView({
  selectedNodeId,
  story,
  onSelectNode,
}: {
  story: RuntimeStory;
  selectedNodeId: string | null;
  onSelectNode: (node: ExecutionNode) => void;
}) {
  const levels = buildLevels(story.nodes);
  const timelineEnd = traceTimelineEnd(story);
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
            {level.map((node) => {
              const left = clampPercent((node.startMs / timelineEnd) * 100);
              const rawWidth = (node.durationMs / timelineEnd) * 100;
              const width = Math.min(Math.max(rawWidth, 3), 100 - left);
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
                    left: `${left}%`,
                    width: `${width}%`,
                  }}
                  type="button"
                >
                  <span className="truncate">
                    {node.name} · {formatTraceDuration(node.durationMs)}
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

function buildLevels(nodes: ExecutionNode[]) {
  const byParent = new Map<string | undefined, ExecutionNode[]>();
  nodes.forEach((node) => {
    const children = byParent.get(node.parentId) ?? [];
    children.push(node);
    byParent.set(node.parentId, children);
  });

  const levels: ExecutionNode[][] = [];
  const visit = (node: ExecutionNode, depth: number) => {
    levels[depth] = [...(levels[depth] ?? []), node];
    (byParent.get(node.id) ?? []).forEach((child) => visit(child, depth + 1));
  };

  (byParent.get(undefined) ?? []).forEach((node) => visit(node, 0));
  return levels;
}
