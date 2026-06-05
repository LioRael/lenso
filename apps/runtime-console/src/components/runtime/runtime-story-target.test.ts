import { describe, expect, test } from "vitest";

import type { RuntimeStory } from "../../data/mock-runtime";
import {
  findStoryByCorrelation,
  resolveRuntimeStoryTarget,
} from "./runtime-story-target";

const story = {
  correlationId: "corr_story",
  durationMs: 42,
  id: "story_id",
  name: "Story",
  nodes: [
    node("evt_1", {}),
    node("fnrun_1", {}),
    node("remoteproxy_rproxy_1", {
      source_metadata: {
        remote_proxy_call_id: "rproxy_1",
        request_id: "req_remote",
      },
    }),
  ],
  service: "api",
  source: "runtime-story",
  status: "failed",
  timestamp: "2026-06-03T00:00:00.000Z",
} satisfies RuntimeStory;

describe("runtime story target resolution", () => {
  test("matches stories by id or correlation id", () => {
    expect(findStoryByCorrelation([story], "story_id")?.id).toBe("story_id");
    expect(findStoryByCorrelation([story], "corr_story")?.id).toBe("story_id");
  });

  test("targets event and function nodes by record id", () => {
    expect(
      resolveRuntimeStoryTarget([story], {
        correlationId: "corr_story",
        nodeIdCandidates: ["fnrun_1"],
      })
    ).toEqual({
      nodeId: "fnrun_1",
      storyId: "story_id",
    });
  });

  test("targets remote proxy nodes by metadata when id candidate is absent", () => {
    expect(
      resolveRuntimeStoryTarget([story], {
        correlationId: "corr_story",
        remoteProxyCallId: "rproxy_1",
        requestId: "req_remote",
      })
    ).toEqual({
      nodeId: "remoteproxy_rproxy_1",
      storyId: "story_id",
    });
  });

  test("falls back to correlation and first node candidate before story data loads", () => {
    expect(
      resolveRuntimeStoryTarget([], {
        correlationId: "corr_late",
        nodeIdCandidates: ["remoteproxy_late"],
      })
    ).toEqual({
      nodeId: "remoteproxy_late",
      storyId: "corr_late",
    });
  });
});

function node(id: string, attributes: Record<string, unknown>) {
  return {
    attributes,
    context: {},
    durationMs: 1,
    events: [],
    id,
    kind: "function",
    logs: [],
    name: id,
    service: "api",
    startMs: 0,
    status: "completed",
  } satisfies RuntimeStory["nodes"][number];
}
