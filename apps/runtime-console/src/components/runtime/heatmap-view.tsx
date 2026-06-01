import type { RuntimeStory, ExecutionNode } from "../../data/mock-runtime";
import { cn } from "../../lib/cn";
import {
  formatRuntimeDuration,
  runtimeTimelineEnd,
} from "../../lib/runtime-style";
import { RuntimeViewHeader } from "./runtime-view-header";

export function HeatmapView({
  selectedNodeId,
  story,
  onSelectNode,
}: {
  story: RuntimeStory;
  selectedNodeId: string | null;
  onSelectNode: (node: ExecutionNode) => void;
}) {
  const timelineEnd = runtimeTimelineEnd(story);
  const cells = Array.from({ length: 120 }, (_, index) => {
    const bucketStart = (index / 120) * timelineEnd;
    const node = story.nodes.find(
      (item) =>
        bucketStart >= item.startMs &&
        bucketStart <= item.startMs + item.durationMs
    );
    return { bucketStart, index, node };
  });

  return (
    <div className="isolate flex h-full min-w-0 flex-col overflow-hidden bg-(--background)">
      <RuntimeViewHeader
        meta="idle · short · work · slow · fault"
        summary={`${cells.length} buckets across ${formatRuntimeDuration(timelineEnd)}`}
        title="Execution Pressure"
      />
      <div className="min-h-0 flex-1 overflow-auto bg-(--background) p-3">
        <div className="grid grid-cols-[repeat(20,minmax(0,1fr))] gap-0.5">
          {cells.map((cell) => (
            <button
              aria-label={
                cell.node
                  ? `Select node ${cell.node.name}`
                  : `Select empty heatmap bucket ${Math.round(cell.bucketStart)}ms`
              }
              className={cn(
                "relative aspect-5/4 rounded-[1px] border border-(--border-subtle) bg-(--elevated) transition hover:z-1 hover:border-(--secondary)",
                cell.node && heatTone(cell.node),
                selectedNodeId === cell.node?.id &&
                  "border-(--accent) outline outline-1 outline-(--accent)"
              )}
              key={cell.index}
              onClick={() => cell.node && onSelectNode(cell.node)}
              title={cell.node?.name ?? formatRuntimeDuration(cell.bucketStart)}
              type="button"
            />
          ))}
        </div>
      </div>
    </div>
  );
}

function heatTone(node: ExecutionNode) {
  if (node.status === "failed" || node.status === "dead") {
    return "bg-[#ef4444]/85";
  }
  if (node.durationMs > 1000) {
    return "bg-[color-mix(in_srgb,var(--accent)_75%,transparent)]";
  }
  if (node.durationMs > 200) {
    return "bg-[#22c55e]/55";
  }
  return "bg-[#3b82f6]/35";
}
