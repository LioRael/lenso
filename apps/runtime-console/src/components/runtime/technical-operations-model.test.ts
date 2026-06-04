import { describe, expect, test } from "vitest";

import type { TechnicalOperation } from "../../data/mock-runtime";
import {
  buildTechnicalOperationGroups,
  technicalOperationSourceLabel,
  technicalOperationSummary,
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
  {
    attributes: {
      declared_path: "/contacts/{id}",
      error_code: "external_dependency_failure",
      method: "GET",
      module_name: "remote-crm",
      remote_path: "/contacts/contact_1",
      remote_status: 502,
      request_id: "req_remote_proxy",
    },
    category: "external",
    correlationId: "corr_1",
    durationMs: 125,
    endedAt: "2026-06-01T10:00:00.425Z",
    id: "remote_proxy:rproxy_1",
    name: "remote-crm GET /contacts/{id}",
    relatedNodeId: "fnrun_1",
    source: "remote_proxy",
    startedAt: "2026-06-01T10:00:00.300Z",
    status: "error",
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

    expect(groups.map((group) => group.label)).toEqual(["db", "external"]);
    expect(groups.find((group) => group.id === "db")).toMatchObject({
      category: "db",
    });
    expect(
      groups
        .find((group) => group.id === "db")
        ?.operations.map((operation) => operation.id)
    ).toEqual(["span_db"]);
    expect(
      groups.find((group) => group.id === "db")?.operations[0]?.relativeStartMs
    ).toBe(100);
    expect(
      groups
        .find((group) => group.id === "external")
        ?.operations.map((operation) => operation.id)
    ).toEqual(["remote_proxy:rproxy_1"]);
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

  test("builds compact remote proxy labels for the renderer", () => {
    const remoteProxy = operations.find(
      (operation) => operation.source === "remote_proxy"
    );

    if (!remoteProxy) {
      throw new Error("remote proxy operation should be present");
    }

    expect(technicalOperationSourceLabel(remoteProxy)).toBe("remote proxy");
    expect(technicalOperationSummary(remoteProxy)).toBe(
      "remote-crm / GET /contacts/{id} / remote /contacts/contact_1 / status 502 / request req_remote_proxy"
    );
  });

  test("builds compact remote runtime labels for the renderer", () => {
    const remoteRuntime: TechnicalOperation = {
      attributes: {
        error_code: "external_dependency_failure",
        function_name: "remote_crm.sync_contact.v1",
        module_name: "remote-crm",
        remote_path: "/runtime/functions/remote_crm.sync_contact.v1/invoke",
        request_id: "fnrun_1",
      },
      category: "external",
      correlationId: "corr_1",
      durationMs: 42,
      endedAt: "2026-06-01T10:00:00.342Z",
      id: "remote_runtime:elog_1",
      name: "remote-crm remote_crm.sync_contact.v1",
      relatedNodeId: "fnrun_1",
      source: "remote_runtime",
      startedAt: "2026-06-01T10:00:00.300Z",
      status: "error",
      storyId: "corr_1",
    };

    expect(technicalOperationSourceLabel(remoteRuntime)).toBe("remote runtime");
    expect(technicalOperationSummary(remoteRuntime)).toBe(
      "remote-crm / remote_crm.sync_contact.v1 / remote /runtime/functions/remote_crm.sync_contact.v1/invoke / duration 42ms / request fnrun_1 / error external_dependency_failure"
    );
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
