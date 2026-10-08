import { expect, test } from "bun:test";
import { definePlugin, startApp } from "@lenso/core";
import { EngineError, diagnostic } from "../src/diagnostics";
import { metrics, trace } from "@opentelemetry/api";
import { spyOn } from "bun:test";
import {
  defineOperation,
  describeOperation,
  validateOperations,
  resolveOperation,
  validateOperationInput,
  invokeValidatedOperation,
  executeOperation,
  type Operation,
} from "../src/operations";

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
    { context: false },
    { confirmation: true },
    { approval: "client-confirmation" },
    { mapError: {} },
  ]) {
    expect(() => validateOperations([plugin], [{ ...operation, ...invalid }])).toThrow();
  }
});

test("shared calls validate raw transformed input once and retain exact service this", async () => {
  let validations = 0;
  let cleanup = 0;
  const transformed = {
    "~standard": {
      version: 1 as const,
      vendor: "test",
      validate(value: unknown) {
        validations++;
        return typeof value === "string"
          ? { value: Number(value) }
          : { issues: [{ message: "Expected raw string", path: ["value"] }] };
      },
      types: undefined as unknown as { input: string; output: number },
    },
  };
  const instance = definePlugin({
    id: "counter:a",
    setup(lifecycle) {
      lifecycle.onCleanup(async () => {
        cleanup++;
      });
      const service = {
        count: 10,
        add(
          value: number,
          context: { evidence: string },
        ): { count: number; evidence: string; secret: string } {
          return { count: this.count + value, evidence: context.evidence, secret: "hidden" };
        },
      };
      return service;
    },
  });
  const operation = defineOperation({
    plugin: instance,
    method: "add",
    input: transformed,
    context: true,
    description: "Add",
  });
  const typed: Operation<{ evidence: string }> = operation;
  expect(typed).toBe(operation);
  const clone = { ...instance };
  expect(() => validateOperations([clone], [operation])).toThrow();
  expect(() => resolveOperation([instance], [], instance.id, "add")).toThrow();
  expect(resolveOperation([instance], [operation], instance.id, "add")).toBe(operation);
  const running = await startApp({ plugins: [instance] });
  try {
    const validated = await validateOperationInput(operation, "2");
    expect(
      await invokeValidatedOperation(running, operation, validated, {
        context: { evidence: "trusted" },
      }),
    ).toEqual({ count: 12, evidence: "trusted", secret: "[REDACTED]" });
    expect(validations).toBe(1);
    expect(cleanup).toBe(0);
    await expect(invokeValidatedOperation(running, operation, validated)).rejects.toMatchObject({
      diagnostic: {
        code: "missing-context-binding",
        instanceId: running.instanceId,
        pluginId: instance.id,
        operation: "counter:a.add",
      },
    });
    await expect(validateOperationInput(operation, 2)).rejects.toMatchObject({
      diagnostic: { code: "invalid-input", phase: "input", details: { paths: [] } },
    });
  } finally {
    await running.stop();
  }
  expect(cleanup).toBe(1);
});

test("trusted gates fail closed and unknown writes are never replayed", async () => {
  let calls = 0;
  const instance = definePlugin({
    id: "writes",
    setup: () => ({
      write(_input: unknown) {
        calls++;
        throw new Error("credential=do-not-disclose");
      },
    }),
  });
  const operation = defineOperation({
    plugin: instance,
    method: "write",
    input,
    description: "Write",
    confirmation: "required",
    approval: "required",
    retry: "safe",
    effect: "write",
  });
  const running = await startApp({ plugins: [instance] });
  try {
    await expect(
      invokeValidatedOperation(running, operation, { confirmed: true }),
    ).rejects.toMatchObject({
      diagnostic: { code: "confirmation-required" },
    });
    await expect(
      invokeValidatedOperation(
        running,
        operation,
        {},
        {
          confirm: () => true,
        },
      ),
    ).rejects.toMatchObject({ diagnostic: { code: "approval-required" } });
    expect(calls).toBe(0);
    try {
      await invokeValidatedOperation(
        running,
        operation,
        {},
        {
          confirm: () => true,
          approve: () => true,
        },
      );
      throw new Error("Expected failure");
    } catch (error) {
      expect(error).toBeInstanceOf(EngineError);
      expect((error as EngineError).diagnostic).toMatchObject({
        code: "invocation-failed",
        phase: "invoke",
        operation: "writes.write",
      });
      expect(JSON.stringify((error as EngineError).diagnostic)).not.toContain("do-not-disclose");
    }
    expect(calls).toBe(1);
  } finally {
    await running.stop();
  }
});

test("finite output rejects streams and applies output limits without stopping a borrowed app", async () => {
  const instance = definePlugin({
    id: "outputs",
    setup: () => ({
      async *stream(_input: unknown) {
        yield { row: 1 };
      },
      large(_input: unknown) {
        return "123456789";
      },
    }),
  });
  const running = await startApp({ plugins: [instance] });
  try {
    const stream = defineOperation({
      plugin: instance,
      method: "stream",
      input,
      description: "Stream",
    });
    const large = defineOperation({
      plugin: instance,
      method: "large",
      input,
      description: "Large",
    });
    await expect(invokeValidatedOperation(running, stream, {})).rejects.toMatchObject({
      diagnostic: { code: "serialization-failed", phase: "output" },
    });
    await expect(
      invokeValidatedOperation(running, large, {}, { maxOutputBytes: 5 }),
    ).rejects.toMatchObject({
      diagnostic: { code: "output-too-large", phase: "output" },
    });
    expect(running.status()).toEqual([{ id: "outputs", state: "ready" }]);
  } finally {
    await running.stop();
  }
});

function operationTypeChecks() {
  const service = definePlugin({
    id: "typed",
    setup: () => ({
      normal(_input: unknown) {
        return 1;
      },
      contextual(_input: unknown, _context: { evidence: string }) {
        return 1;
      },
      wrongInput(_input: number, _context: { evidence: string }) {
        return 1;
      },
    }),
  });
  defineOperation({ plugin: service, method: "normal", input, description: "Normal" });
  defineOperation({
    plugin: service,
    // @ts-expect-error Context declarations require an actual second method parameter.
    method: "normal",
    input,
    context: true,
    description: "Normal",
  });
  // @ts-expect-error A required second argument needs an explicit context declaration.
  defineOperation({ plugin: service, method: "contextual", input, description: "Contextual" });
  const contextual = defineOperation({
    plugin: service,
    method: "contextual",
    input,
    context: true,
    description: "Contextual",
  });
  contextual satisfies Operation<{ evidence: string }>;
  // @ts-expect-error The context type comes from the real method.
  contextual satisfies Operation<{ evidence: number }>;
  defineOperation({
    plugin: service,
    // @ts-expect-error The shared schema's validated output must match the actual method input.
    method: "wrongInput",
    input,
    context: true,
    description: "Wrong",
  });
}
void operationTypeChecks;

test("domain projectors keep original causes and do not classify output or gate failures", async () => {
  class DomainError extends Error {}
  const original = new DomainError("private input");
  const trusted = new EngineError({ code: "custom-code", phase: "invoke", message: "Safe." });
  let thrown: unknown = original;
  let mappings = 0;
  const instance = definePlugin({
    id: "domain",
    setup: () => ({
      run(_input: unknown) {
        if (thrown) throw thrown;
        return undefined;
      },
    }),
  });
  const operation = defineOperation({
    plugin: instance,
    method: "run",
    input,
    description: "Run",
    mapError(error) {
      mappings++;
      return error instanceof DomainError
        ? { code: "not-found", phase: "invoke", message: "Resource not found." }
        : undefined;
    },
  });
  const running = await startApp({ plugins: [instance] });
  try {
    const failure = await invokeValidatedOperation(running, operation, {}).catch((error) => error);
    expect((failure as EngineError).cause).toBe(original);
    expect(diagnostic(failure)).toMatchObject({ code: "not-found" });
    expect(diagnostic(failure).causes).toBeUndefined();
    thrown = trusted;
    expect(await invokeValidatedOperation(running, operation, {}).catch((error) => error)).toBe(
      trusted,
    );
    thrown = { code: "not-found", message: "private" };
    expect(
      await invokeValidatedOperation(running, operation, {}).catch((error) => error),
    ).toMatchObject({ diagnostic: { code: "invocation-failed" }, cause: thrown });
    thrown = original;
    expect(
      await invokeValidatedOperation(
        running,
        {
          ...operation,
          mapError() {
            throw new Error();
          },
        },
        {},
      ).catch((error) => error),
    ).toMatchObject({ cause: original });
    const before = mappings;
    thrown = null;
    await expect(invokeValidatedOperation(running, operation, {})).rejects.toMatchObject({
      diagnostic: { code: "serialization-failed" },
    });
    await expect(
      invokeValidatedOperation(running, { ...operation, approval: "required" }, {}),
    ).rejects.toMatchObject({ diagnostic: { code: "approval-required" } });
    expect(mappings).toBe(before);
  } finally {
    await running.stop();
  }
});

test("input paths omit dynamic keys and runtime-only schema paths", async () => {
  const operation = defineOperation({
    plugin,
    method: "cancel",
    description: "Cancel",
    input: {
      "~standard": {
        version: 1,
        vendor: "test",
        jsonSchema: {
          input: () => ({
            type: "object",
            properties: {
              name: { type: "string" },
              record: { type: "object", additionalProperties: { type: "string" } },
            },
          }),
        },
        validate: () => ({
          issues: [
            { message: "private", path: ["name"] },
            { message: "private", path: ["record", "secret-user-key"] },
            { message: "private", path: ["unknown-secret"] },
          ],
        }),
      },
    },
  });
  const error = await validateOperationInput(operation, {}).catch((cause) => cause);
  expect((error as EngineError).diagnostic.details).toEqual({ paths: [["name"]] });
});

test("throwing telemetry cannot replace success or original service failure", async () => {
  const original = new Error("business");
  let fail = false;
  let calls = 0;
  const instance = definePlugin({
    id: "telemetry",
    setup: () => ({
      run(_input: unknown) {
        calls++;
        if (fail) throw original;
        return 7;
      },
    }),
  });
  const operation = defineOperation({ plugin: instance, method: "run", input, description: "Run" });
  const running = await startApp({ plugins: [instance] });
  const meter = spyOn(metrics, "getMeter").mockImplementation(() => {
    throw new Error("meter");
  });
  const tracer = spyOn(trace, "getTracer").mockImplementation(() => {
    throw new Error("tracer");
  });
  try {
    expect(await executeOperation(running, operation, {})).toBe(7);
    fail = true;
    expect(await executeOperation(running, operation, {}).catch((error) => error)).toBe(original);
    expect(calls).toBe(2);
  } finally {
    meter.mockRestore();
    tracer.mockRestore();
    await running.stop();
  }
});
