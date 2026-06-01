import type {
  ExecutionEdge,
  ExecutionNode,
  RuntimeStory,
} from "../../data/mock-runtime";

export type RuntimeGraphModel = {
  edges: ExecutionEdge[];
  source: "backend" | "derived";
  state: "ready" | "empty-nodes" | "missing-edges";
};

export type RuntimeGraphLayoutNode = {
  node: ExecutionNode;
  depth: number;
  row: number;
  parentId?: string;
};

export function buildRuntimeGraphModel(story: RuntimeStory): RuntimeGraphModel {
  if (story.edges !== undefined) {
    return {
      edges: story.edges,
      source: "backend",
      state:
        story.nodes.length > 0 && story.edges.length === 0
          ? "missing-edges"
          : "ready",
    };
  }

  return {
    edges: edgesFromParents(story.nodes),
    source: "derived",
    state: story.nodes.length === 0 ? "empty-nodes" : "ready",
  };
}

export function buildRuntimeGraphLayout(story: RuntimeStory): {
  edges: ExecutionEdge[];
  nodes: RuntimeGraphLayoutNode[];
} {
  const graph = buildRuntimeGraphModel(story);
  const depthById = nodeDepths(story.nodes, graph.edges);
  const nodesById = new Map(story.nodes.map((node) => [node.id, node]));
  const childrenByParent = new Map<string, ExecutionNode[]>();
  const parentByNode = new Map<string, string>();

  for (const edge of graph.edges) {
    const parent = nodesById.get(edge.source);
    const child = nodesById.get(edge.target);
    if (!parent || !child || parentByNode.has(child.id)) {
      continue;
    }

    parentByNode.set(child.id, parent.id);
    childrenByParent.set(parent.id, [
      ...(childrenByParent.get(parent.id) ?? []),
      child,
    ]);
  }

  for (const children of childrenByParent.values()) {
    children.sort(compareNodesByStart);
  }

  const connectedNodeIds = new Set<string>();
  for (const [parentId, children] of childrenByParent) {
    connectedNodeIds.add(parentId);
    for (const child of children) {
      connectedNodeIds.add(child.id);
    }
  }

  const roots = story.nodes
    .filter(
      (node) => connectedNodeIds.has(node.id) && !parentByNode.has(node.id)
    )
    .sort(compareNodesByStart);
  const layoutNodes: RuntimeGraphLayoutNode[] = [];
  const visited = new Set<string>();

  const visit = (node: ExecutionNode) => {
    if (visited.has(node.id)) {
      return;
    }
    visited.add(node.id);
    const parentId = parentByNode.get(node.id);
    layoutNodes.push({
      depth: depthById.get(node.id) ?? 0,
      node,
      ...(parentId === undefined ? {} : { parentId }),
      row: layoutNodes.length,
    });

    for (const child of childrenByParent.get(node.id) ?? []) {
      visit(child);
    }
  };

  for (const root of roots) {
    visit(root);
  }

  for (const node of story.nodes
    .filter((candidate) => !visited.has(candidate.id))
    .sort(compareNodesByStart)) {
    const parentId = parentByNode.get(node.id);
    layoutNodes.push({
      depth: depthById.get(node.id) ?? 0,
      node,
      ...(parentId === undefined ? {} : { parentId }),
      row: layoutNodes.length,
    });
  }

  return { edges: graph.edges, nodes: layoutNodes };
}

function edgesFromParents(nodes: ExecutionNode[]): ExecutionEdge[] {
  return nodes
    .filter((node): node is ExecutionNode & { parentId: string } =>
      Boolean(node.parentId)
    )
    .map((node) => ({
      id: `${node.parentId}:${node.id}:parent`,
      source: node.parentId,
      target: node.id,
      type: "sequence",
    }));
}

function nodeDepths(
  nodes: ExecutionNode[],
  edges: Array<{ source: string; target: string }>
) {
  const parentsByTarget = new Map<string, string[]>();
  for (const edge of edges) {
    parentsByTarget.set(edge.target, [
      ...(parentsByTarget.get(edge.target) ?? []),
      edge.source,
    ]);
  }

  const depthById = new Map<string, number>();
  const visit = (id: string, seen = new Set<string>()): number => {
    if (depthById.has(id)) {
      return depthById.get(id)!;
    }
    if (seen.has(id)) {
      return 0;
    }
    const nextSeen = new Set(seen).add(id);
    const parents = parentsByTarget.get(id) ?? [];
    const depth =
      parents.length === 0
        ? 0
        : Math.max(...parents.map((parentId) => visit(parentId, nextSeen))) + 1;
    depthById.set(id, depth);
    return depth;
  };

  for (const node of nodes) {
    visit(node.id);
  }
  return depthById;
}

function compareNodesByStart(left: ExecutionNode, right: ExecutionNode) {
  return left.startMs - right.startMs || left.name.localeCompare(right.name);
}
