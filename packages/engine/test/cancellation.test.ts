import { expect, test } from "bun:test";
import { definePlugin, startApp } from "@lenso/core";
import { diagnostic, EngineError } from "../src/diagnostics";
import {
  defineOperation,
  executeOperation,
  invokeValidatedOperation,
  type Operation,
} from "../src/operations";

const input = {
  "~standard": {
    version: 1 as const,
    vendor: "cancellation-test",
    validate: (value: unknown) => ({ value }),
  },
};

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

function expectAborted(error: unknown, reason: unknown) {
  expect(error).toBeInstanceOf(EngineError);
  expect((error as EngineError).diagnostic).toMatchObject({ code: "aborted", phase: "invoke" });
  expect(Object.is((error as EngineError).cause, reason)).toBe(true);
  const safe = diagnostic(error);
  expect(safe.causes).toBeUndefined();
  expect(JSON.stringify(safe)).not.toContain("private-reason");
}

test("preaborted real signals produce safe aborted diagnostics and never invoke business", async () => {
  let calls = 0;
  const plugin = definePlugin({
    id: "preabort",
    setup: () => ({
      run(_input: unknown) {
        calls++;
        return true;
      },
    }),
  });
  const operation = defineOperation({ plugin, method: "run", input, description: "Run" });
  const running = await startApp({ plugins: [plugin] });
  try {
    const reasons = [
      new DOMException("private-reason", "AbortError"),
      new Error("private-reason"),
      "private-reason",
      { message: "private-reason", code: "arbitrary" },
      null,
      0,
      false,
      NaN,
      new EngineError({ code: "private-reason", phase: "invoke", message: "private-reason" }),
      new AggregateError([new Error("private-reason")], "private-reason"),
    ];
    for (const reason of reasons) {
      const controller = new AbortController();
      controller.abort(reason);
      const error = await invokeValidatedOperation(
        running,
        operation,
        {},
        {
          signal: controller.signal,
        },
      ).catch((cause: unknown) => cause);
      expect(calls).toBe(0);
      expectAborted(error, reason);
    }
    const controller = new AbortController();
    controller.abort();
    expectAborted(
      await invokeValidatedOperation(running, operation, {}, { signal: controller.signal }).catch(
        (cause: unknown) => cause,
      ),
      controller.signal.reason,
    );
    expect(calls).toBe(0);
  } finally {
    await running.stop();
  }
});

for (const gate of ["confirm", "approve"] as const) {
  test(`abort across async ${gate} waits for the gate and prevents business dispatch`, async () => {
    const entered = deferred<void>();
    const finish = deferred<boolean>();
    const controller = new AbortController();
    const reason = { message: "private-reason" };
    let calls = 0;
    const plugin = definePlugin({
      id: `abort-${gate}`,
      setup: () => ({
        run(_input: unknown) {
          calls++;
          return true;
        },
      }),
    });
    const operation: Operation = {
      plugin,
      method: "run",
      input,
      description: "Run",
      ...(gate === "confirm" ? { confirmation: "required" } : { approval: "required" }),
    };
    const running = await startApp({ plugins: [plugin] });
    try {
      let settled = false;
      const invocation = invokeValidatedOperation(
        running,
        operation,
        {},
        {
          signal: controller.signal,
          [gate]: () => {
            entered.resolve();
            return finish.promise;
          },
        },
      ).catch((cause: unknown) => cause);
      void invocation.then(() => {
        settled = true;
      });
      await entered.promise;
      controller.abort(reason);
      await Bun.sleep(0);
      expect(settled).toBe(false);
      expect(calls).toBe(0);
      finish.resolve(true);
      expectAborted(await invocation, reason);
      expect(calls).toBe(0);
    } finally {
      finish.resolve(true);
      await running.stop();
    }
  });
}

test("executing cancellation waits for settlement and classifies only the original signal reason", async () => {
  for (const outcome of [
    "success",
    "reason",
    "business",
    "spoofed-name",
    "spoofed-code",
  ] as const) {
    const entered = deferred<void>();
    const finish = deferred<void>();
    const controller = new AbortController();
    const reason = { message: "private-reason" };
    const business = new Error("private-business");
    const spoofedName = new DOMException("private-business", "AbortError");
    const spoofedCode = { name: "AbortError", code: "ABORT_ERR", message: "private-business" };
    let cleanup = 0;
    let projections = 0;
    const plugin = definePlugin({
      id: `executing-${outcome}`,
      setup(lifecycle) {
        lifecycle.onCleanup(() => {
          cleanup++;
        });
        return {
          async run(_input: unknown) {
            entered.resolve();
            await finish.promise;
            if (outcome === "reason") controller.signal.throwIfAborted();
            if (outcome === "business") throw business;
            if (outcome === "spoofed-name") throw spoofedName;
            if (outcome === "spoofed-code") throw spoofedCode;
            return true;
          },
        };
      },
    });
    const operation = defineOperation({
      plugin,
      method: "run",
      input,
      description: "Run",
      mapError(error) {
        projections++;
        return error === business || error === reason
          ? { code: "conflict", phase: "invoke", message: "Conflict." }
          : undefined;
      },
    });
    const running = await startApp({ plugins: [plugin] });
    try {
      let settled = false;
      const invocation = invokeValidatedOperation(
        running,
        operation,
        {},
        {
          signal: controller.signal,
        },
      ).catch((cause: unknown) => cause);
      void invocation.then(() => {
        settled = true;
      });
      await entered.promise;
      controller.abort(reason);
      await Bun.sleep(0);
      expect(settled).toBe(false);
      expect(cleanup).toBe(0);
      finish.resolve();
      const error = await invocation;
      if (outcome === "success" || outcome === "reason") {
        expectAborted(error, reason);
        expect(projections).toBe(0);
      } else {
        expect(error).toBeInstanceOf(EngineError);
        expect((error as EngineError).diagnostic.code).toBe(
          outcome === "business" ? "conflict" : "invocation-failed",
        );
        expect((error as EngineError).cause).toBe(
          outcome === "business"
            ? business
            : outcome === "spoofed-name"
              ? spoofedName
              : spoofedCode,
        );
        expect(projections).toBe(1);
      }
      expect(cleanup).toBe(0);
    } finally {
      finish.resolve();
      await running.stop();
    }
    expect(cleanup).toBe(1);
  }
});

test("AbortError names and aborted codes without an aborted signal are business failures", async () => {
  const failures = [
    new DOMException("private-business", "AbortError"),
    { code: "aborted", name: "AbortError", message: "private-business" },
  ];
  for (const failure of failures) {
    const plugin = definePlugin({
      id: "spoofed",
      setup: () => ({
        run(_input: unknown) {
          throw failure;
        },
      }),
    });
    const operation = defineOperation({ plugin, method: "run", input, description: "Run" });
    const running = await startApp({ plugins: [plugin] });
    try {
      const error = await invokeValidatedOperation(
        running,
        operation,
        {},
        {
          signal: new AbortController().signal,
        },
      ).catch((cause: unknown) => cause);
      expect((error as EngineError).diagnostic.code).toBe("invocation-failed");
      expect((error as EngineError).cause).toBe(failure);
    } finally {
      await running.stop();
    }
  }
});

test("equal primitive rejection reasons do not prove cancellation provenance", async () => {
  for (const reason of ["same", null, false, 0, NaN]) {
    const controller = new AbortController();
    const plugin = definePlugin({
      id: "primitive-business",
      setup: () => ({
        run(_input: unknown) {
          controller.abort(reason);
          throw reason;
        },
      }),
    });
    const operation = defineOperation({ plugin, method: "run", input, description: "Run" });
    const running = await startApp({ plugins: [plugin] });
    try {
      const error = await invokeValidatedOperation(
        running,
        operation,
        {},
        {
          signal: controller.signal,
        },
      ).catch((cause: unknown) => cause);
      expect((error as EngineError).diagnostic.code).toBe("invocation-failed");
      expect(Object.is((error as EngineError).cause, reason)).toBe(true);
    } finally {
      await running.stop();
    }
  }
});

test("final entry revalidation follows gates and rechecks cancellation before dispatch", async () => {
  const events: string[] = [];
  const controller = new AbortController();
  const reason = "private-reason";
  const plugin = definePlugin({
    id: "final-admission",
    setup: () => ({
      run(_input: unknown) {
        events.push("run");
        return true;
      },
    }),
  });
  const operation = defineOperation({
    plugin,
    method: "run",
    input,
    description: "Run",
    confirmation: "required",
    approval: "required",
  });
  const running = await startApp({ plugins: [plugin] });
  try {
    const error = await invokeValidatedOperation(
      running,
      operation,
      {},
      {
        signal: controller.signal,
        async confirm() {
          events.push("confirm");
          return true;
        },
        async approve() {
          events.push("approve");
          return true;
        },
        async beforeExecute() {
          events.push("revalidate");
          await Promise.resolve();
          controller.abort(reason);
        },
      },
    ).catch((cause: unknown) => cause);
    expect(events).toEqual(["confirm", "approve", "revalidate"]);
    expectAborted(error, reason);
  } finally {
    await running.stop();
  }
});

test("entry revalidation failures bypass domain projectors; direct execution still uses them", async () => {
  const original = new Error("private-business");
  let projections = 0;
  let calls = 0;
  const plugin = definePlugin({
    id: "entry-projection",
    setup: () => ({
      run(_input: unknown) {
        calls++;
        throw original;
      },
    }),
  });
  const operation = defineOperation({
    plugin,
    method: "run",
    input,
    description: "Run",
    mapError(error) {
      projections++;
      return error === original
        ? { code: "conflict", phase: "invoke", message: "Conflict." }
        : undefined;
    },
  });
  const running = await startApp({ plugins: [plugin] });
  try {
    const gateFailure = await invokeValidatedOperation(
      running,
      operation,
      {},
      {
        beforeExecute: () => {
          throw original;
        },
      },
    ).catch((cause: unknown) => cause);
    expect((gateFailure as EngineError).diagnostic.code).toBe("invocation-failed");
    expect((gateFailure as EngineError).cause).toBe(original);
    expect(projections).toBe(0);
    expect(calls).toBe(0);
    const businessFailure = await executeOperation(running, operation, {}).catch(
      (cause: unknown) => cause,
    );
    expect((businessFailure as EngineError).diagnostic.code).toBe("conflict");
    expect((businessFailure as EngineError).cause).toBe(original);
    expect(projections).toBe(1);
    expect(calls).toBe(1);
  } finally {
    await running.stop();
  }
});

test("async confirmation rejection attributes exact reason identity but preserves real failures", async () => {
  for (const outcome of ["reason", "business"] as const) {
    const entered = deferred<void>();
    const finish = deferred<void>();
    const controller = new AbortController();
    const reason = new Error("private-reason");
    const business = new Error("private-business");
    let calls = 0;
    let projections = 0;
    const plugin = definePlugin({
      id: "confirmation-rejection",
      setup: () => ({
        run(_input: unknown) {
          calls++;
          return true;
        },
      }),
    });
    const operation = defineOperation({
      plugin,
      method: "run",
      input,
      description: "Run",
      confirmation: "required",
      mapError() {
        projections++;
        return undefined;
      },
    });
    const running = await startApp({ plugins: [plugin] });
    try {
      const call = invokeValidatedOperation(
        running,
        operation,
        {},
        {
          signal: controller.signal,
          async confirm() {
            entered.resolve();
            await finish.promise;
            if (outcome === "reason") controller.signal.throwIfAborted();
            throw business;
          },
        },
      ).catch((cause: unknown) => cause);
      await entered.promise;
      controller.abort(reason);
      finish.resolve();
      const error = await call;
      if (outcome === "reason") expectAborted(error, reason);
      else {
        expect((error as EngineError).diagnostic.code).toBe("invocation-failed");
        expect((error as EngineError).cause).toBe(business);
      }
      expect(projections).toBe(0);
      expect(calls).toBe(0);
    } finally {
      finish.resolve();
      await running.stop();
    }
  }
});
