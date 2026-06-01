import type {
  ExecutionNode,
  RuntimeStatus,
  RuntimeStory,
  TimelineItem,
} from "../../data/mock-runtime";
import { buildParallelExecutionGroups } from "./parallel-execution-model";
import { buildRuntimeGraphModel } from "./runtime-graph-model";

export type WaterfallTimelineMarker = {
  id: string;
  kind: TimelineItem["type"];
  name: string;
  status: RuntimeStatus;
  startMs: number;
  durationMs: number;
};

export type WaterfallRow = {
  id: string;
  kind: ExecutionNode["kind"] | TimelineItem["type"];
  name: string;
  status: RuntimeStatus;
  service: string;
  startMs: number;
  durationMs: number;
  depth: number;
  parentId?: string;
  node?: ExecutionNode;
  markers: WaterfallTimelineMarker[];
  group?: "linked" | "unlinked";
  fanoutGroupSize?: number;
  parallelGroupSize?: number;
};

export function buildWaterfallRows(story: RuntimeStory): WaterfallRow[] {
  const nodeRows = buildNodeRows(story);
  const attachedMarkerIds = new Set<string>();
  const rowsByNodeId = new Map(
    nodeRows.map((row) => [row.node?.id ?? row.id, row])
  );

  for (const item of story.timelineItems ?? []) {
    const node = findNodeForTimelineItem(story, item);
    if (!node) {
      continue;
    }

    const row = rowsByNodeId.get(node.id);
    if (!row) {
      continue;
    }

    row.markers.push(markerFromTimelineItem(story, item, node));
    attachedMarkerIds.add(item.id);
  }

  for (const row of nodeRows) {
    row.markers.sort(compareWaterfallMarkers);
  }

  const unlinkedRows =
    story.timelineItems
      ?.filter((item) => !attachedMarkerIds.has(item.id))
      .map((item, index) => rowFromUnlinkedTimelineItem(story, item, index))
      .sort(compareWaterfallRows) ?? [];

  return [...nodeRows, ...unlinkedRows];
}

export function findExecutionNodeForWaterfallRow(row: WaterfallRow) {
  return row.node ?? null;
}

export function waterfallTimelineEnd(story: RuntimeStory) {
  const rows = buildWaterfallRows(story);
  const latestRowEnd = Math.max(
    0,
    ...rows.map((row) => row.startMs + row.durationMs),
    ...rows.flatMap((row) =>
      row.markers.map((marker) => marker.startMs + marker.durationMs)
    )
  );
  return Math.max(story.durationMs, latestRowEnd, 1);
}

function buildNodeRows(story: RuntimeStory): WaterfallRow[] {
  const graph = buildRuntimeGraphModel(story);
  const parallelGroups = buildParallelExecutionGroups(story);
  const fanoutGroupSizeByParent = new Map(
    parallelGroups.map((group) => [group.parentId, group.branchCount])
  );
  const parallelGroupSizeByChild = new Map(
    parallelGroups.flatMap((group) =>
      group.childIds.map((childId) => [childId, group.branchCount] as const)
    )
  );
  const childrenByParent = new Map<string, ExecutionNode[]>();
  const parentByNode = new Map<string, string>();

  for (const edge of graph.edges) {
    const parent = story.nodes.find((node) => node.id === edge.source);
    const child = story.nodes.find((node) => node.id === edge.target);
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
  const rows: WaterfallRow[] = [];
  const visited = new Set<string>();

  const visit = (node: ExecutionNode, depth: number) => {
    if (visited.has(node.id)) {
      return;
    }
    visited.add(node.id);
    const parentId = parentByNode.get(node.id);
    const fanoutGroupSize = fanoutGroupSizeByParent.get(node.id);
    const parallelGroupSize = parallelGroupSizeByChild.get(node.id);
    rows.push(
      rowFromNode(node, depth, "linked", {
        ...(fanoutGroupSize === undefined ? {} : { fanoutGroupSize }),
        ...(parallelGroupSize === undefined ? {} : { parallelGroupSize }),
        ...(parentId === undefined ? {} : { parentId }),
      })
    );
    for (const child of childrenByParent.get(node.id) ?? []) {
      visit(child, depth + 1);
    }
  };

  for (const root of roots) {
    visit(root, 0);
  }

  const orphanNodes = story.nodes
    .filter((node) => !visited.has(node.id))
    .sort(compareNodesByStart);
  for (const node of orphanNodes) {
    const parentId = parentByNode.get(node.id);
    const fanoutGroupSize = fanoutGroupSizeByParent.get(node.id);
    const parallelGroupSize = parallelGroupSizeByChild.get(node.id);
    rows.push(
      rowFromNode(node, 0, "unlinked", {
        ...(fanoutGroupSize === undefined ? {} : { fanoutGroupSize }),
        ...(parallelGroupSize === undefined ? {} : { parallelGroupSize }),
        ...(parentId === undefined ? {} : { parentId }),
      })
    );
  }

  return rows;
}

function rowFromNode(
  node: ExecutionNode,
  depth: number,
  group: "linked" | "unlinked",
  metadata: {
    fanoutGroupSize?: number;
    parallelGroupSize?: number;
    parentId?: string;
  } = {}
): WaterfallRow {
  return {
    durationMs: node.durationMs,
    ...(metadata.fanoutGroupSize === undefined
      ? {}
      : { fanoutGroupSize: metadata.fanoutGroupSize }),
    id: node.id,
    kind: node.kind,
    markers: [],
    name: node.name,
    node,
    ...(metadata.parallelGroupSize === undefined
      ? {}
      : { parallelGroupSize: metadata.parallelGroupSize }),
    ...(metadata.parentId === undefined ? {} : { parentId: metadata.parentId }),
    service: node.service,
    startMs: node.startMs,
    status: node.status,
    depth,
    group,
  };
}

function rowFromUnlinkedTimelineItem(
  story: RuntimeStory,
  item: TimelineItem,
  index: number
): WaterfallRow {
  const marker = markerFromTimelineItem(story, item);
  return {
    durationMs: marker.durationMs,
    group: "unlinked",
    id: `unlinked:${item.id}`,
    kind: item.type,
    markers: [marker],
    name: item.name,
    service: serviceFromTimelineType(item.type),
    startMs: marker.startMs + index / 1000,
    status: item.status,
    depth: 0,
  };
}

function markerFromTimelineItem(
  story: RuntimeStory,
  item: TimelineItem,
  node?: ExecutionNode
): WaterfallTimelineMarker {
  const startMs = item.startedAt
    ? offsetMs(story.timestamp, item.startedAt, node?.startMs ?? 0)
    : offsetMs(story.timestamp, item.createdAt, node?.startMs ?? 0);
  const endMs = item.completedAt
    ? offsetMs(story.timestamp, item.completedAt, startMs)
    : startMs + (node?.durationMs ?? 1);

  return {
    durationMs: Math.max(0, endMs - startMs),
    id: item.id,
    kind: item.type,
    name: item.name,
    startMs,
    status: item.status,
  };
}

function findNodeForTimelineItem(story: RuntimeStory, item: TimelineItem) {
  return (
    story.nodes.find((node) => node.id === item.detailId) ??
    story.nodes.find((node) => node.id === item.id) ??
    null
  );
}

function compareWaterfallRows(left: WaterfallRow, right: WaterfallRow) {
  return left.startMs - right.startMs || left.name.localeCompare(right.name);
}

function compareWaterfallMarkers(
  left: WaterfallTimelineMarker,
  right: WaterfallTimelineMarker
) {
  return left.startMs - right.startMs || left.name.localeCompare(right.name);
}

function compareNodesByStart(left: ExecutionNode, right: ExecutionNode) {
  return left.startMs - right.startMs || left.name.localeCompare(right.name);
}

function offsetMs(baseTimestamp: string, timestamp: string, fallback: number) {
  const base = Date.parse(baseTimestamp);
  const value = Date.parse(timestamp);
  if (Number.isFinite(base) && Number.isFinite(value)) {
    return Math.max(0, value - base);
  }
  return fallback;
}

function serviceFromTimelineType(type: TimelineItem["type"]) {
  if (type === "outbox_event") {
    return "outbox";
  }
  if (type === "function_run") {
    return "runtime.functions";
  }
  if (type === "http_request") {
    return "http";
  }
  return "runtime";
}
