import { describe, expect, test } from "vitest";

import type { TechnicalOperation } from "../../data/mock-runtime";
import {
  buildTechnicalOperationGroups,
  technicalOperationsStateLabel,
} from "./technical-operations-model";

const operations: TechnicalOperation[] = [
  {
    attributes: {
      "db.system": "postgresql",
      "db.statement": "select * from users",
      "lenso.function_run_id": "fnrun_1",
    },
    category: "db",
    correlationId: "corr_1",
    durationMs: 25,
    endedAt: "2026-06-01T10:00:00.125Z",
    id: "span_db",
    name: "SELECT identity.users",
    relatedNodeId: "fnrun_1",
    source: "otel",
    startedAt: "2026-06-01T10:00:00.100Z",
    status: "ok",
    storyId: "corr_1",
  },
  {
    attributes: {
      "http.request.header.authorization": "Bearer secret",
      "http.request.method": "GET",
    },
    category: "http",
    correlationId: "corr_1",
    durationMs: 50,
    endedAt: "2026-06-01T10:00:00.250Z",
    id: "span_http",
    name: "GET https://example.test/resources",
    source: "otel",
    startedAt: "2026-06-01T10:00:00.200Z",
    status: "ok",
    storyId: "corr_1",
  },
];

describe("technical operations model", () => {
  test("supports the story-to-drawer technical operation smoke path", () => {
    const selectedNodeId = "fnrun_1";
    const storyStartedAt = "2026-06-01T10:00:00.000Z";
    const groups = buildTechnicalOperationGroups({
      executionOperations: operations,
      selectedNodeId,
      storyOperations: operations,
      storyTimestamp: storyStartedAt,
    });

    const executionOperation = groups
      .flatMap((group) => group.operations)
      .find((operation) => operation.id === "span_db");
    const storyLevelOperation = groups
      .find((group) => group.id === "story-level")
      ?.operations.find((operation) => operation.id === "span_http");

    expect(executionOperation).toMatchObject({
      category: "db",
      name: "SELECT identity.users",
      relatedNodeId: selectedNodeId,
      relativeStartMs: 100,
      status: "ok",
    });
    expect(storyLevelOperation).toMatchObject({
      category: "http",
      name: "GET https://example.test/resources",
      relativeStartMs: 200,
      status: "ok",
    });
  });

  test("groups selected execution operations by category", () => {
    const groups = buildTechnicalOperationGroups({
      executionOperations: operations,
      selectedNodeId: "fnrun_1",
      storyOperations: [],
      storyTimestamp: "2026-06-01T10:00:00.000Z",
    });

    expect(groups).toHaveLength(1);
    expect(groups[0]).toMatchObject({
      category: "db",
      label: "db",
    });
    expect(groups[0]?.operations.map((operation) => operation.id)).toEqual([
      "span_db",
    ]);
    expect(groups[0]?.operations[0]?.relativeStartMs).toBe(100);
  });

  test("keeps unlinked operations under story-level operations", () => {
    const groups = buildTechnicalOperationGroups({
      executionOperations: [],
      selectedNodeId: "fnrun_1",
      storyOperations: operations,
      storyTimestamp: "2026-06-01T10:00:00.000Z",
    });

    expect(groups.map((group) => group.label)).toEqual([
      "Story-level operations",
    ]);
    expect(groups[0]?.operations.map((operation) => operation.id)).toEqual([
      "span_http",
    ]);
  });

  test("does not expose unsafe attributes to the renderer", () => {
    const groups = buildTechnicalOperationGroups({
      executionOperations: operations,
      selectedNodeId: "fnrun_1",
      storyOperations: operations,
      storyTimestamp: "2026-06-01T10:00:00.000Z",
    });

    const attributes = groups.flatMap((group) =>
      group.operations.map((operation) => operation.safeAttributes)
    );

    expect(attributes).toEqual(
      expect.arrayContaining([
        {
          "db.system": "postgresql",
          "lenso.function_run_id": "fnrun_1",
        },
        {
          "http.request.method": "GET",
        },
      ])
    );
    expect(JSON.stringify(attributes)).not.toContain("Bearer secret");
    expect(JSON.stringify(attributes)).not.toContain("select * from users");
  });

  test("returns execution-specific empty and error copy", () => {
    expect(
      technicalOperationsStateLabel({
        error: null,
        isError: false,
        isLoading: true,
      })
    ).toBe("Loading technical operations...");
    expect(
      technicalOperationsStateLabel({
        error: null,
        isError: false,
        isLoading: false,
      })
    ).toBe("No technical operations recorded for this execution.");
    expect(
      technicalOperationsStateLabel({
        error: new Error("backend unavailable"),
        isError: true,
        isLoading: false,
      })
    ).toBe("Technical operations could not be loaded.");
  });
});
