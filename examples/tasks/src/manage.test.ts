import { expect, test } from "bun:test";
import { invoke } from "@lenso/cli";
import { describeManage, selectManageOperations } from "@lenso/manage";
import definition, { manage, operations, mcpOperations } from "../lenso.config";
import { createTasksOperations, taskOperations, tasks } from "./plugin";

test("Tasks CLI and MCP select original finite declarations without enabling report management", () => {
  expect(definition.plugins).toContain(tasks);
  expect(manage).toEqual([taskOperations.manage]);
  expect(
    describeManage(taskOperations.manage).operations.map((operation) => operation.method),
  ).toEqual(["submit", "query", "cancel", "retry"]);
  expect(operations.map((operation) => operation.method)).toEqual([
    "submit",
    "query",
    "cancel",
    "retry",
    "report",
  ]);
  expect(mcpOperations.map((operation) => operation.method)).toEqual([
    "submit",
    "query",
    "cancel",
    "retry",
  ]);
  const agent = selectManageOperations(taskOperations.manage, ["query"]);
  const original = taskOperations.operations.find((operation) => operation.method === "query")!;
  expect(agent[0]!).toBe(original);
  expect(operations.find((operation) => operation.method === "query")).toBe(original);
  expect(mcpOperations.find((operation) => operation.method === "query")).toBe(original);
  expect(() => selectManageOperations(taskOperations.manage, ["report"])).toThrow();
  expect(
    taskOperations.operations.find((operation) => operation.method === "cancel"),
  ).toMatchObject({ cancellation: "request-only", destructive: true });
});

test("Tasks selected input validation rejects identity/gates before authentication or resources", async () => {
  let connects = 0;
  const fixture = createTasksOperations({
    evidence: () => null,
    async connectAuth() {
      connects++;
      throw new Error("Invalid inputs must never reach setup.");
    },
  });
  const app = {
    plugins: [fixture.plugin],
    operations: selectManageOperations(fixture.manage, ["submit", "query"]),
  };
  const valid = { reportId: "manage-input", rows: [1, 2] };
  for (const input of [
    { ...valid, actor: { subjectId: "alice" } },
    { ...valid, confirmed: true },
    { ...valid, approved: true },
    { ...valid, evidence: "session" },
    { ...valid, rows: ["bad"] },
  ]) {
    await expect(invoke(app, fixture.plugin.id, "submit", input)).rejects.toMatchObject({
      diagnostic: { code: "invalid-input" },
    });
  }
  await expect(
    invoke(app, fixture.plugin.id, "cancel", { jobId: crypto.randomUUID() }),
  ).rejects.toMatchObject({ diagnostic: { code: "unknown-operation" } });
  expect(connects).toBe(0);
});
