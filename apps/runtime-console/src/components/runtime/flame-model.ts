import type { ExecutionNode } from "../../data/mock-runtime";

export function buildFlameLevels(nodes: ExecutionNode[]) {
  const byParent = new Map<string | undefined, ExecutionNode[]>();
  const nodeIds = new Set(nodes.map((node) => node.id));
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

  nodes
    .filter((node) => !node.parentId || !nodeIds.has(node.parentId))
    .sort(compareNodesByStart)
    .forEach((node) => visit(node, 0));
  return levels;
}

function compareNodesByStart(left: ExecutionNode, right: ExecutionNode) {
  return left.startMs - right.startMs || left.name.localeCompare(right.name);
}
