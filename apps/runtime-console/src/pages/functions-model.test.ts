import { describe, expect, test } from "vitest";

import type { FunctionRun } from "../data/mock-runtime";
import {
  aggregateFunctionRuns,
  distinctFunctionMetadata,
  filterFunctionRuns,
  formatFunctionDuration,
  functionInspectorDetails,
  functionPrimarySummary,
  functionStatusTone,
  runDurationMs,
  summarizeFunctionRuns,
} from "./functions-model";

const runs: FunctionRun[] = [
  functionRun({
    completedAt: "2026-06-03T00:00:01.200Z",
    correlationId: "corr_remote",
    functionName: "remote_crm.sync_contact.v1",
    id: "fn_remote",
    runtimeDeclaration: {
      moduleName: "remote-crm",
      moduleSource: "remote",
      name: "remote_crm.sync_contact.v1",
      queue: "remote-crm",
      version: 1,
    },
    startedAt: "2026-06-03T00:00:00.000Z",
    status: "completed",
  }),
  functionRun({
    completedAt: "2026-06-03T00:00:03.000Z",
    correlationId: "corr_identity",
    functionName: "identity.audit_user.v1",
    id: "fn_identity",
    lastError: "permission denied",
    runtimeDeclaration: {
      moduleName: "identity",
      moduleSource: "linked",
      name: "identity.audit_user.v1",
      queue: "identity",
      version: 1,
    },
    startedAt: "2026-06-03T00:00:00.000Z",
    status: "failed",
  }),
  functionRun({
    completedAt: "2026-06-03T00:00:02.000Z",
    correlationId: "corr_identity",
    createdAt: "2026-06-03T00:01:00.000Z",
    functionName: "identity.sync_user.v1",
    id: "fn_identity_dead",
    runtimeDeclaration: {
      moduleName: "identity",
      moduleSource: "linked",
      name: "identity.sync_user.v1",
      queue: "identity",
      version: 1,
    },
    startedAt: "2026-06-03T00:00:00.000Z",
    status: "dead",
  }),
];

describe("functions model", () => {
  test("filters by status, module, queue, and search terms", () => {
    expect(
      filterFunctionRuns(runs, {
        moduleName: "identity",
        query: "sync corr_identity",
        queue: "identity",
        status: "dead",
      }).map((run) => run.id)
    ).toEqual(["fn_identity_dead"]);
  });

  test("summarizes function run status counts", () => {
    expect(summarizeFunctionRuns(runs)).toMatchObject({
      completed: 1,
      dead: 1,
      failed: 1,
      total: 3,
    });
  });

  test("aggregates function runs by module with failures first", () => {
    expect(aggregateFunctionRuns(runs, "module", 5)).toEqual([
      {
        avgDurationMs: 2500,
        dead: 1,
        failed: 1,
        key: "identity",
        total: 2,
      },
      {
        avgDurationMs: 1200,
        dead: 0,
        failed: 0,
        key: "remote-crm",
        total: 1,
      },
    ]);
  });

  test("lists declared module and queue filters", () => {
    expect(distinctFunctionMetadata(runs, "module")).toEqual([
      "identity",
      "remote-crm",
    ]);
    expect(distinctFunctionMetadata(runs, "queue")).toEqual([
      "identity",
      "remote-crm",
    ]);
  });

  test("formats function durations", () => {
    expect(runDurationMs(runs[0]!)).toBe(1200);
    expect(formatFunctionDuration(42)).toBe("42ms");
    expect(formatFunctionDuration(1200)).toBe("1.2s");
  });

  test("derives inspector status tone and primary summaries", () => {
    expect(functionStatusTone("completed")).toBe("success");
    expect(functionStatusTone("running")).toBe("warning");
    expect(functionStatusTone("failed")).toBe("warning");
    expect(functionStatusTone("dead")).toBe("error");
    expect(functionPrimarySummary(runs[0]!)).toBe(
      "remote_crm.sync_contact.v1 / remote-crm / 1/3"
    );
    expect(functionPrimarySummary(runs[1]!)).toBe("permission denied");
  });

  test("builds inspector run context and lineage rows", () => {
    const details = functionInspectorDetails(runs[0]!, {
      actor: "system",
      duration: "1.2s",
    });

    expect(details.statusTone).toBe("success");
    expect(details.runRows).toContainEqual([
      "function",
      "remote_crm.sync_contact.v1",
    ]);
    expect(details.runRows).toContainEqual(["module", "remote-crm"]);
    expect(details.runRows).toContainEqual(["duration", "1.2s"]);
    expect(details.lineageRows).toContainEqual(["id", "fn_remote"]);
    expect(details.lineageRows).toContainEqual(["correlation", "corr_remote"]);
    expect(details.lineageRows).toContainEqual(["actor", "system"]);
  });
});

function functionRun(overrides: Partial<FunctionRun>): FunctionRun {
  return {
    actor: { kind: "system" },
    attempts: 1,
    correlationId: "corr_test",
    createdAt: "2026-06-03T00:00:00.000Z",
    functionName: "test.fn.v1",
    id: "fn_test",
    input: {},
    logs: [],
    maxAttempts: 3,
    status: "completed",
    ...overrides,
  };
}
