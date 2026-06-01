import type {
  RuntimeStatus,
  RuntimeStory,
  ExecutionNode,
} from "../data/mock-runtime";

const serviceColors = [
  "#f3f724",
  "#22c55e",
  "#3b82f6",
  "#a855f7",
  "#f97316",
  "#14b8a6",
  "#ec4899",
  "#94a3b8",
] as const;

export function formatRuntimeDuration(ms: number) {
  if (ms < 1) {
    return `${Math.round(ms * 1000)}us`;
  }
  if (ms < 1000) {
    return `${Math.round(ms)}ms`;
  }
  return `${(ms / 1000).toFixed(2)}s`;
}

export function statusColor(status: RuntimeStatus) {
  if (status === "failed" || status === "dead") {
    return "#ef4444";
  }
  if (status === "pending" || status === "processing" || status === "running") {
    return "#f3f724";
  }
  return "#22c55e";
}

export function serviceColor(service: string) {
  const hash = [...service].reduce(
    (total, char) => total + (char.codePointAt(0) ?? 0),
    0
  );
  return serviceColors[hash % serviceColors.length];
}

export function runtimeStoryStats(story: RuntimeStory) {
  const errors = story.nodes.filter(
    (node) => node.status === "failed" || node.status === "dead"
  );
  const services = Array.from(new Set(story.nodes.map((node) => node.service)));
  return {
    errors: errors.length,
    services,
    nodeCount: story.nodes.length,
  };
}

export function runtimeTimelineEnd(story: RuntimeStory) {
  const latestNodeEnd = Math.max(
    0,
    ...story.nodes.map((node) => node.startMs + node.durationMs)
  );
  return Math.max(story.durationMs, latestNodeEnd, 1);
}

export function nodeDepth(node: ExecutionNode, nodes: ExecutionNode[]) {
  let depth = 0;
  let { parentId } = node;
  while (parentId) {
    const currentParentId = parentId;
    const parent = nodes.find((item) => item.id === currentParentId);
    if (!parent) {
      break;
    }
    depth += 1;
    ({ parentId } = parent);
  }
  return depth;
}

export function criticalPath(story: RuntimeStory) {
  const byParent = new Map<string | undefined, ExecutionNode[]>();
  story.nodes.forEach((node) => {
    byParent.set(node.parentId, [...(byParent.get(node.parentId) ?? []), node]);
  });

  const path: ExecutionNode[] = [];
  const roots = [...(byParent.get(undefined) ?? [])].sort(
    (left, right) => right.durationMs - left.durationMs
  );
  let [current] = roots;

  while (current) {
    path.push(current);
    const children = [...(byParent.get(current.id) ?? [])].sort(
      (left, right) => right.durationMs - left.durationMs
    );
    [current] = children;
  }

  return path;
}
