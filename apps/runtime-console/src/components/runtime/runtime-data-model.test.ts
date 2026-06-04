import { describe, expect, test } from "vitest";

import { runtimeStories, type RuntimeStory } from "../../data/mock-runtime";
import { timelineSegmentLayout } from "../../lib/runtime-style";
import {
  buildExecutionTimelineRows,
  findExecutionNodeForRow,
} from "./execution-timeline-model";
import { buildFlameLevels } from "./flame-model";
import { resolveHeatmapCellNodes } from "./heatmap-model";
import {
  buildParallelExecutionGroups,
  buildTimelineParallelMarkers,
} from "./parallel-execution-model";
import {
  buildRuntimeGraphLayout,
  buildRuntimeGraphModel,
} from "./runtime-graph-model";
import {
  buildWaterfallRows,
  findExecutionNodeForWaterfallRow,
  waterfallSegmentLayout,
} from "./waterfall-model";

const story: RuntimeStory = {
  correlationId: "corr_test",
  durationMs: 240,
  edges: [
    {
      id: "edge_backend",
      source: "node_a",
      target: "node_b",
      type: "causation",
    },
  ],
  id: "corr_test",
  name: "Test story",
  nodes: [
    {
      attributes: {},
      context: {},
      durationMs: 90,
      events: [],
      id: "node_a",
      kind: "function",
      logs: [],
      name: "CreateResource",
      service: "resources",
      startMs: 0,
      status: "completed",
    },
    {
      attributes: {},
      context: {},
      durationMs: 120,
      events: [],
      id: "node_b",
      kind: "event",
      logs: [],
      name: "resource.created.v1",
      parentId: "node_a",
      service: "outbox",
      startMs: 100,
      status: "published",
    },
  ],
  service: "runtime",
  source: "runtime-story",
  status: "completed",
  timelineItems: [
    {
      attempts: 1,
      completedAt: "2026-06-01T00:00:00.090Z",
      correlationId: "corr_test",
      createdAt: "2026-06-01T00:00:00.000Z",
      detailId: "node_a",
      id: "timeline_function",
      maxAttempts: 1,
      name: "CreateResource",
      startedAt: "2026-06-01T00:00:00.000Z",
      status: "completed",
      type: "function_run",
    },
    {
      attempts: 2,
      completedAt: "2026-06-01T00:00:00.220Z",
      correlationId: "corr_test",
      createdAt: "2026-06-01T00:00:00.100Z",
      detailId: "node_b",
      id: "timeline_retry",
      maxAttempts: 3,
      name: "resource.created.v1",
      startedAt: "2026-06-01T00:00:00.100Z",
      status: "published",
      type: "retry",
    },
  ],
  timestamp: "2026-06-01T00:00:00.000Z",
};

describe("runtime story data model", () => {
  test("uses backend edges without deriving parent-order edges", () => {
    const model = buildRuntimeGraphModel(story);

    expect(model.source).toBe("backend");
    expect(model.edges).toEqual(story.edges);
  });

  test("only derives graph edges when backend edges are missing", () => {
    const { edges: _edges, ...storyWithoutEdges } = story;
    const model = buildRuntimeGraphModel(storyWithoutEdges);

    expect(model.source).toBe("derived");
    expect(model.edges).toEqual([
      {
        id: "node_a:node_b:parent",
        source: "node_a",
        target: "node_b",
        type: "sequence",
      },
    ]);
  });

  test("reports backend stories with nodes but no edges", () => {
    const model = buildRuntimeGraphModel({ ...story, edges: [] });

    expect(model.source).toBe("backend");
    expect(model.state).toBe("missing-edges");
    expect(model.edges).toEqual([]);
  });

  test("renders backend timeline items directly and preserves item kind", () => {
    const rows = buildExecutionTimelineRows(story);

    expect(rows.map((row) => row.id)).toEqual([
      "timeline_function",
      "timeline_retry",
    ]);
    expect(rows.map((row) => row.kind)).toEqual(["function_run", "retry"]);
    expect(rows[1]?.source).toBe("backend");
    expect(rows[1]?.durationMs).toBe(120);
  });

  test("keeps remote proxy calls in backend timeline rows", () => {
    const remoteStory: RuntimeStory = {
      ...story,
      nodes: [
        ...story.nodes,
        {
          attributes: {
            source_metadata: {
              module_name: "remote-crm",
              remote_status: 200,
            },
          },
          context: {},
          durationMs: 42,
          events: [],
          id: "remoteproxy_rproxy_1",
          kind: "external",
          logs: [],
          name: "Fetch Contact",
          parentId: "node_a",
          service: "remote-crm",
          startMs: 180,
          status: "completed",
        },
      ],
      timelineItems: [
        ...(story.timelineItems ?? []),
        {
          attempts: 1,
          completedAt: "2026-06-01T00:00:00.222Z",
          correlationId: "corr_test",
          createdAt: "2026-06-01T00:00:00.180Z",
          detailId: "remoteproxy_rproxy_1",
          id: "remoteproxy_rproxy_1",
          maxAttempts: 1,
          name: "Fetch Contact",
          startedAt: "2026-06-01T00:00:00.180Z",
          status: "completed",
          type: "remote_proxy_call",
        },
      ],
    };

    const rows = buildExecutionTimelineRows(remoteStory);
    const remoteRow = rows.find((row) => row.id === "remoteproxy_rproxy_1");

    expect(remoteRow).toMatchObject({
      durationMs: 42,
      kind: "remote_proxy_call",
      name: "Fetch Contact",
      service: "remote-crm",
      source: "backend",
      status: "completed",
    });
    expect(findExecutionNodeForRow(remoteStory, remoteRow!)?.id).toBe(
      "remoteproxy_rproxy_1"
    );
  });

  test("maps timeline rows back to execution nodes for shared selection", () => {
    const rows = buildExecutionTimelineRows(story);

    expect(findExecutionNodeForRow(story, rows[0]!)?.id).toBe("node_a");
    expect(findExecutionNodeForRow(story, rows[1]!)?.id).toBe("node_b");
  });

  test("falls back to execution nodes when backend timeline items are missing", () => {
    const { timelineItems: _timelineItems, ...storyWithoutTimelineItems } =
      story;
    const rows = buildExecutionTimelineRows(storyWithoutTimelineItems);

    expect(rows.map((row) => row.id)).toEqual(["node_a", "node_b"]);
    expect(rows.every((row) => row.source === "node")).toBe(true);
  });

  test("builds nested waterfall rows from parent and child edges", () => {
    const rows = buildWaterfallRows(story);

    expect(rows.slice(0, 2).map((row) => [row.id, row.depth])).toEqual([
      ["node_a", 0],
      ["node_b", 1],
    ]);
  });

  test("attaches backend timeline items to matching waterfall node rows", () => {
    const rows = buildWaterfallRows(story);

    expect(rows[0]?.markers.map((marker) => marker.id)).toEqual([
      "timeline_function",
    ]);
    expect(rows[1]?.markers.map((marker) => marker.kind)).toEqual(["retry"]);
    expect(rows[1]?.markers[0]?.durationMs).toBe(120);
    expect(rows[1]?.markers[0]?.matchesRowTiming).toBe(true);
  });

  test("waterfall marks timeline markers with timing drift", () => {
    const rows = buildWaterfallRows({
      ...story,
      timelineItems: [
        {
          attempts: 1,
          completedAt: "2026-06-01T00:00:00.160Z",
          correlationId: "corr_test",
          createdAt: "2026-06-01T00:00:00.020Z",
          detailId: "node_a",
          id: "timeline_drift",
          maxAttempts: 1,
          name: "CreateResource drift",
          startedAt: "2026-06-01T00:00:00.020Z",
          status: "completed",
          type: "function_run",
        },
      ],
    });

    expect(rows[0]?.markers[0]).toMatchObject({
      durationMs: 140,
      matchesRowTiming: false,
      startMs: 20,
    });
  });

  test("keeps orphan timeline items as unlinked waterfall rows", () => {
    const rows = buildWaterfallRows({
      ...story,
      timelineItems: [
        ...(story.timelineItems ?? []),
        {
          attempts: 1,
          completedAt: "2026-06-01T00:00:00.235Z",
          correlationId: "corr_test",
          createdAt: "2026-06-01T00:00:00.230Z",
          detailId: "missing_node",
          id: "timeline_orphan",
          maxAttempts: 1,
          name: "orphan failure",
          startedAt: "2026-06-01T00:00:00.230Z",
          status: "failed",
          type: "failure",
        },
      ],
    });

    expect(rows.at(-1)).toMatchObject({
      group: "unlinked",
      id: "unlinked:timeline_orphan",
      kind: "failure",
    });
  });

  test("orders connected hierarchy before unlinked waterfall rows", () => {
    const rows = buildWaterfallRows({
      ...story,
      nodes: [
        ...story.nodes,
        {
          attributes: {},
          context: {},
          durationMs: 25,
          events: [],
          id: "node_orphan",
          kind: "external",
          logs: [],
          name: "orphan provider call",
          service: "provider",
          startMs: 10,
          status: "completed",
        },
      ],
    });

    expect(rows.map((row) => row.id)).toEqual([
      "node_a",
      "node_b",
      "node_orphan",
    ]);
    expect(rows.at(-1)?.group).toBe("unlinked");
  });

  test("selecting a waterfall row resolves to matching execution detail", () => {
    const rows = buildWaterfallRows(story);

    expect(findExecutionNodeForWaterfallRow(rows[1]!)?.id).toBe("node_b");
    expect(findExecutionNodeForWaterfallRow(rows.at(-1)!)).not.toBeNull();
  });

  test("waterfall segment layout preserves duration and clips at timeline end", () => {
    expect(
      timelineSegmentLayout({
        durationMs: 120,
        minWidthPercent: 0.8,
        startMs: 100,
        timelineEnd: 240,
      })
    ).toEqual({
      left: 41.666_666_666_666_67,
      width: 50,
    });

    expect(
      waterfallSegmentLayout({
        durationMs: 120,
        minWidthPercent: 0.8,
        startMs: 100,
        timelineEnd: 240,
      })
    ).toEqual({
      left: 41.666_666_666_666_67,
      width: 50,
    });

    expect(
      waterfallSegmentLayout({
        durationMs: 20,
        minWidthPercent: 0.8,
        startMs: 230,
        timelineEnd: 240,
      }).width
    ).toBeCloseTo((10 / 240) * 100);
  });

  test("flame levels keep nodes whose parent is missing", () => {
    const missingParentNode: RuntimeStory["nodes"][number] = {
      attributes: {},
      context: {},
      durationMs: 25,
      events: [],
      id: "node_missing_parent",
      kind: "external",
      logs: [],
      name: "missing parent child",
      parentId: "missing_parent",
      service: "provider",
      startMs: 20,
      status: "completed",
    };
    const levels = buildFlameLevels([...story.nodes, missingParentNode]);

    expect(levels.flat().map((node) => node.id)).toContain(
      "node_missing_parent"
    );
  });

  test("maps story heatmap cells back to matching execution nodes", () => {
    const nodes = resolveHeatmapCellNodes({
      cell: {
        bucketEnd: "2026-06-01T00:00:01.000Z",
        bucketStart: "2026-06-01T00:00:00.000Z",
        deadCount: 0,
        errorCount: 0,
        nodeType: "event",
        service: "outbox",
        totalCount: 1,
      },
      story,
    });

    expect(nodes.map((node) => node.id)).toEqual(["node_b"]);
  });

  test("keeps story heatmap drilldown scoped to the bucket window", () => {
    const nodes = resolveHeatmapCellNodes({
      cell: {
        bucketEnd: "2026-06-01T00:00:00.080Z",
        bucketStart: "2026-06-01T00:00:00.000Z",
        deadCount: 0,
        errorCount: 0,
        nodeType: "event",
        service: "outbox",
        totalCount: 1,
      },
      story,
    });

    expect(nodes).toEqual([]);
  });

  test("mock fan-out story has parallel siblings under the published event", () => {
    const fanoutStory = runtimeStories.find(
      (item) => item.id === "story_resource_published_fanout"
    )!;
    const siblingIds = [
      "generate_search_index",
      "sync_cdn_metadata",
      "send_subscriber_notifications",
    ];

    expect(
      siblingIds.map(
        (id) => fanoutStory.nodes.find((node) => node.id === id)?.parentId
      )
    ).toEqual([
      "resource_version_published",
      "resource_version_published",
      "resource_version_published",
    ]);
  });

  test("mock fan-out sibling execution durations overlap", () => {
    const fanoutStory = runtimeStories.find(
      (item) => item.id === "story_resource_published_fanout"
    )!;
    const search = fanoutStory.nodes.find(
      (node) => node.id === "generate_search_index"
    )!;
    const cdn = fanoutStory.nodes.find(
      (node) => node.id === "sync_cdn_metadata"
    )!;
    const notifications = fanoutStory.nodes.find(
      (node) => node.id === "send_subscriber_notifications"
    )!;

    expect(overlaps(search, cdn)).toBe(true);
    expect(overlaps(search, notifications)).toBe(true);
    expect(overlaps(cdn, notifications)).toBe(true);
  });

  test("mock fan-out waterfall siblings share the same depth", () => {
    const fanoutStory = runtimeStories.find(
      (item) => item.id === "story_resource_published_fanout"
    )!;
    const rows = buildWaterfallRows(fanoutStory);
    const depths = [
      "generate_search_index",
      "sync_cdn_metadata",
      "send_subscriber_notifications",
    ].map((id) => rows.find((row) => row.id === id)?.depth);

    expect(depths).toEqual([3, 3, 3]);
  });

  test("mock fan-out graph has three outgoing child edges from the event", () => {
    const fanoutStory = runtimeStories.find(
      (item) => item.id === "story_resource_published_fanout"
    )!;
    const graph = buildRuntimeGraphModel(fanoutStory);
    const outgoingTargets = graph.edges
      .filter((edge) => edge.source === "resource_version_published")
      .map((edge) => edge.target)
      .sort();

    expect(outgoingTargets).toEqual([
      "generate_search_index",
      "mark_publish_complete",
      "send_subscriber_notifications",
      "sync_cdn_metadata",
    ]);
    expect(
      outgoingTargets.filter((target) =>
        [
          "generate_search_index",
          "sync_cdn_metadata",
          "send_subscriber_notifications",
        ].includes(target)
      )
    ).toHaveLength(3);
  });

  test("detects parallel execution group from fan-out siblings", () => {
    const fanoutStory = runtimeStories.find(
      (item) => item.id === "story_resource_published_fanout"
    )!;
    const groups = buildParallelExecutionGroups(fanoutStory);

    expect(groups).toHaveLength(1);
    expect(groups[0]).toMatchObject({
      branchCount: 3,
      longestBranchId: "send_subscriber_notifications",
      parentId: "resource_version_published",
    });
    expect(groups[0]?.childIds).toEqual([
      "generate_search_index",
      "send_subscriber_notifications",
      "sync_cdn_metadata",
    ]);
  });

  test("graph layout keeps parallel fan-out children as same-depth siblings", () => {
    const fanoutStory = runtimeStories.find(
      (item) => item.id === "story_resource_published_fanout"
    )!;
    const layout = buildRuntimeGraphLayout(fanoutStory);
    const siblingRows = [
      "generate_search_index",
      "sync_cdn_metadata",
      "send_subscriber_notifications",
    ].map((id) => layout.nodes.find((node) => node.node.id === id)!);

    expect(siblingRows.map((row) => row.depth)).toEqual([3, 3, 3]);
    expect(new Set(siblingRows.map((row) => row.depth)).size).toBe(1);
    expect(siblingRows.map((row) => row.parentId)).toEqual([
      "resource_version_published",
      "resource_version_published",
      "resource_version_published",
    ]);
  });

  test("waterfall annotates parallel fan-out child rows", () => {
    const fanoutStory = runtimeStories.find(
      (item) => item.id === "story_resource_published_fanout"
    )!;
    const rows = buildWaterfallRows(fanoutStory);
    const siblingRows = [
      "generate_search_index",
      "sync_cdn_metadata",
      "send_subscriber_notifications",
    ].map((id) => rows.find((row) => row.id === id)!);

    expect(siblingRows.map((row) => row.parallelGroupSize)).toEqual([3, 3, 3]);
    expect(siblingRows.map((row) => row.parentId)).toEqual([
      "resource_version_published",
      "resource_version_published",
      "resource_version_published",
    ]);
  });

  test("timeline exposes a grouped parallel start marker", () => {
    const fanoutStory = runtimeStories.find(
      (item) => item.id === "story_resource_published_fanout"
    )!;
    const markers = buildTimelineParallelMarkers(fanoutStory);

    expect(markers).toEqual([
      {
        branchCount: 3,
        firstNodeId: "generate_search_index",
        id: "parallel:resource_version_published:2000",
        label: "3 parallel executions started",
        parentId: "resource_version_published",
        startMs: 2000,
      },
    ]);
  });
});

function overlaps(
  left: { startMs: number; durationMs: number },
  right: { startMs: number; durationMs: number }
) {
  return (
    left.startMs < right.startMs + right.durationMs &&
    right.startMs < left.startMs + left.durationMs
  );
}
