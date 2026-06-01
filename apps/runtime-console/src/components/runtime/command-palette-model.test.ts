import { describe, expect, test, vi } from "vitest";

import type { RuntimeStory } from "../../data/mock-runtime";
import { buildStoryCommandItems } from "./command-palette-model";

const backendStories = [
  {
    correlationId: "corr_backend_story",
    durationMs: 25,
    id: "story_backend_story",
    name: "Backend Story",
    nodes: [
      {
        attributes: {},
        context: {},
        durationMs: 15,
        events: [],
        id: "fn_backend_email",
        kind: "function",
        logs: [],
        name: "SendBackendEmail",
        service: "notifications",
        startMs: 0,
        status: "completed",
      },
      {
        attributes: {},
        context: {},
        durationMs: 10,
        events: [],
        id: "evt_backend_registered",
        kind: "event",
        logs: [],
        name: "identity.user_registered",
        service: "identity",
        startMs: 15,
        status: "completed",
      },
    ],
    service: "notifications",
    source: "api",
    status: "completed",
    timestamp: "2026-06-01T00:00:00.000Z",
  },
] satisfies RuntimeStory[];

describe("command palette model", () => {
  test("builds story commands from query-backed stories", () => {
    const openTimeline = vi.fn();
    const commands = buildStoryCommandItems({
      onOpenStory: openTimeline,
      stories: backendStories,
    });

    expect(commands).toHaveLength(1);
    expect(commands[0]).toMatchObject({
      id: "story:corr_backend_story",
      subtitle: "completed · corr_backend_story",
      title: "Backend Story",
    });

    commands[0]?.action();
    expect(openTimeline).toHaveBeenCalledWith("story_backend_story");
  });

  test("makes event and function node names searchable", () => {
    const commands = buildStoryCommandItems({
      onOpenStory: vi.fn(),
      stories: backendStories,
    });

    expect(commands[0]?.searchText).toContain("sendbackendemail");
    expect(commands[0]?.searchText).toContain("identity.user_registered");
  });
});
