import { expect, test } from "bun:test";
import { definePlugin } from "@lenso/core";
import { defineOperation, describeOperation, validateOperations } from "../src/operations";

const input = {
  "~standard": {
    version: 1 as const,
    vendor: "test",
    validate: (value: unknown) => ({ value }),
  },
};
const plugin = definePlugin({
  id: "jobs",
  setup: () => ({ cancel: async (_input: unknown) => "requested" }),
});

test("canonical descriptions retain semantics without inventing a JSON schema", () => {
  const operation = defineOperation({
    plugin,
    method: "cancel",
    input,
    description: "Request cancellation",
    effect: "write",
    destructive: false,
    outputDescription: "requested does not mean the handler stopped",
    retry: "safe",
    cancellation: "request-only",
    source: { file: "src/jobs.ts", export: "jobs" },
  });
  expect(validateOperations([plugin], [operation])).toEqual([operation]);
  expect(describeOperation(operation, "lenso.config.ts")).toMatchObject({
    pluginId: "jobs",
    method: "cancel",
    destructive: false,
    outputDescription: operation.outputDescription,
    retry: "safe",
    cancellation: "request-only",
    source: operation.source,
    inputSchema: null,
    schemaAvailability: "runtime-validation-only",
  });
});

test("omitted semantics stay unknown; malformed hints fail discovery", () => {
  const operation = defineOperation({
    plugin,
    method: "cancel",
    input,
    description: "Cancel",
  });
  expect(describeOperation(operation, "config.ts")).toMatchObject({
    destructive: null,
    outputDescription: null,
    retry: "unknown",
    cancellation: "unknown",
  });
  for (const invalid of [
    { effect: "delete" },
    { destructive: "false" },
    { outputDescription: {} },
    { retry: "automatic" },
    { cancellation: "rollback" },
  ]) {
    expect(() => validateOperations([plugin], [{ ...operation, ...invalid }])).toThrow();
  }
});
