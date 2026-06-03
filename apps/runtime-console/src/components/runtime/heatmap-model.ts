import type { ExecutionNode, RuntimeStory } from "../../data/mock-runtime";
import type { RuntimeHeatmapCell } from "../../hooks/use-runtime-queries";

export function heatmapCellKey(cell: RuntimeHeatmapCell, index: number) {
  return `${cell.bucketStart}:${cell.service}:${cell.nodeType}:${index}`;
}

export function resolveHeatmapCellNodes({
  cell,
  story,
}: {
  cell: RuntimeHeatmapCell;
  story: RuntimeStory;
}): ExecutionNode[] {
  const bucketStartMs = Date.parse(cell.bucketStart);
  const bucketEndMs = Date.parse(cell.bucketEnd);
  const storyStartMs = Date.parse(story.timestamp);
  const hasTimeRange =
    Number.isFinite(bucketStartMs) &&
    Number.isFinite(bucketEndMs) &&
    Number.isFinite(storyStartMs) &&
    bucketEndMs > bucketStartMs;

  return story.nodes
    .filter((node) => {
      if (node.service !== cell.service) {
        return false;
      }
      if (heatmapNodeType(node) !== cell.nodeType) {
        return false;
      }
      if (!hasTimeRange) {
        return true;
      }

      const nodeStartedAt = storyStartMs + node.startMs;
      return nodeStartedAt >= bucketStartMs && nodeStartedAt < bucketEndMs;
    })
    .sort((left, right) => left.startMs - right.startMs);
}

function heatmapNodeType(node: ExecutionNode): RuntimeHeatmapCell["nodeType"] {
  if (node.kind === "event") {
    return "event";
  }
  if (node.kind === "http") {
    return "http";
  }
  return "function";
}
