import { expect, spyOn, test } from "bun:test";
import { RuntimeError, RuntimeHost } from "@lenso/runtime-sdk";
import { operations } from "../lenso.config";
import { aggregate, EmptyBatchError } from "./aggregate";
import { runtimeWorkspace } from "./workspace";

test("native TS aggregation shares the batch output and rejects empty batches", () => {
  expect(
    aggregate({
      logs: [
        { level: "error", message: "synthetic failure", duration_ms: 12 },
        { level: "info", message: "synthetic request", duration_ms: 8 },
      ],
    }),
  ).toEqual({ count: 2, error_count: 1, total_duration_ms: 20 });
  expect(() => aggregate({ logs: [] })).toThrow(EmptyBatchError);
});

test("CLI exposes declared Runtime failure codes, not remote messages or arbitrary codes", () => {
  const map = operations[0]!.mapError!;
  expect(map(new RuntimeError("deadline_exceeded", "secret input"))).toEqual({
    code: "deadline_exceeded",
    phase: "invoke",
    message: "The Runtime call exceeded its deadline.",
  });
  expect(map(new RuntimeError("contract_mismatch", "secret input"))?.code).toBe(
    "contract_mismatch",
  );
  expect(map(new RuntimeError("unknown-secret-code", "secret input"))).toBeUndefined();
  expect(map(new Error("secret input"))).toBeUndefined();
});

test("a fresh old-selection snapshot does not prove a timed-out promotion was rejected", async () => {
  const connection = spyOn(RuntimeHost, "connect");
  const operator = {
    async promote() {
      throw new RuntimeError("connection_lost", "connection_lost");
    },
    close() {},
  } as unknown as RuntimeHost;
  const observer = {
    async status() {
      return {
        services: [
          { namespace: "demo", service: "batch", instance: "old", selected: true, state: "ready" },
        ],
      };
    },
    close() {},
  } as unknown as RuntimeHost;
  connection.mockResolvedValueOnce(operator).mockResolvedValueOnce(observer);
  try {
    const workspace = runtimeWorkspace({ runtimeRoot: "/runtime", rustRoot: "/rust" });
    const service = workspace.services[1]!;
    if (service.kind !== "process") throw new Error("Expected a process service");
    await expect(
      service.activate!({
        instance: "candidate",
        signal: new AbortController().signal,
        process: {} as Bun.Subprocess,
      }),
    ).rejects.toMatchObject({ diagnostic: { code: "workspace-cutover-unknown" } });
  } finally {
    connection.mockRestore();
  }
});
