import type { ExecutionNode, RuntimeStory } from "../../data/mock-runtime";

export type ParallelExecutionGroup = {
  id: string;
  parentId: string;
  childIds: string[];
  branchCount: number;
  startMs: number;
  longestBranchId: string;
  longestBranchName: string;
};

export type TimelineParallelMarker = {
  id: string;
  parentId: string;
  firstNodeId: string;
  branchCount: number;
  startMs: number;
  label: string;
};

const parallelStartWindowMs = 350;

export function buildParallelExecutionGroups(
  story: RuntimeStory
): ParallelExecutionGroup[] {
  const nodesById = new Map(story.nodes.map((node) => [node.id, node]));
  const childrenByParent = childNodesByParent(story, nodesById);
  const groups: ParallelExecutionGroup[] = [];

  for (const [parentId, children] of childrenByParent) {
    const sortedChildren = [...children].sort(compareNodesByStart);
    for (const cluster of clusterByStartWindow(
      sortedChildren,
      parallelStartWindowMs
    )) {
      const parallelChildren = cluster.filter((child) =>
        cluster.some(
          (candidate) => candidate.id !== child.id && overlaps(child, candidate)
        )
      );
      if (parallelChildren.length < 2) {
        continue;
      }

      const [longestBranch] = [...parallelChildren].sort(
        (left, right) =>
          right.durationMs - left.durationMs ||
          right.startMs + right.durationMs - (left.startMs + left.durationMs) ||
          left.name.localeCompare(right.name)
      );
      if (!longestBranch) {
        continue;
      }

      const startMs = Math.min(
        ...parallelChildren.map((child) => child.startMs)
      );
      groups.push({
        branchCount: parallelChildren.length,
        childIds: parallelChildren.map((child) => child.id),
        id: `parallel:${parentId}:${startMs}`,
        longestBranchId: longestBranch.id,
        longestBranchName: longestBranch.name,
        parentId,
        startMs,
      });
    }
  }

  return groups.sort(
    (left, right) =>
      left.startMs - right.startMs ||
      left.parentId.localeCompare(right.parentId)
  );
}

export function buildTimelineParallelMarkers(
  story: RuntimeStory
): TimelineParallelMarker[] {
  return buildParallelExecutionGroups(story).map((group) => ({
    branchCount: group.branchCount,
    firstNodeId: group.childIds[0]!,
    id: group.id,
    label: `${group.branchCount} parallel executions started`,
    parentId: group.parentId,
    startMs: group.startMs,
  }));
}

function childNodesByParent(
  story: RuntimeStory,
  nodesById: Map<string, ExecutionNode>
) {
  const childrenByParent = new Map<string, ExecutionNode[]>();

  if (story.edges !== undefined) {
    for (const edge of story.edges) {
      const child = nodesById.get(edge.target);
      if (!child || !nodesById.has(edge.source)) {
        continue;
      }
      childrenByParent.set(edge.source, [
        ...(childrenByParent.get(edge.source) ?? []),
        child,
      ]);
    }
    return childrenByParent;
  }

  for (const child of story.nodes) {
    if (!child.parentId || !nodesById.has(child.parentId)) {
      continue;
    }
    childrenByParent.set(child.parentId, [
      ...(childrenByParent.get(child.parentId) ?? []),
      child,
    ]);
  }

  return childrenByParent;
}

function clusterByStartWindow(nodes: ExecutionNode[], windowMs: number) {
  const clusters: ExecutionNode[][] = [];
  let current: ExecutionNode[] = [];
  let windowStart = 0;

  for (const node of nodes) {
    if (current.length === 0) {
      current = [node];
      windowStart = node.startMs;
      continue;
    }

    if (node.startMs - windowStart <= windowMs) {
      current.push(node);
      continue;
    }

    clusters.push(current);
    current = [node];
    windowStart = node.startMs;
  }

  if (current.length > 0) {
    clusters.push(current);
  }

  return clusters;
}

function compareNodesByStart(left: ExecutionNode, right: ExecutionNode) {
  return left.startMs - right.startMs || left.name.localeCompare(right.name);
}

function overlaps(left: ExecutionNode, right: ExecutionNode) {
  return (
    left.startMs < right.startMs + right.durationMs &&
    right.startMs < left.startMs + left.durationMs
  );
}
