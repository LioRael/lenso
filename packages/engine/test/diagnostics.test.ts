import { expect, test } from "bun:test";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { EngineSession, withEngine } from "../src/engine-host";
import {
  EngineError,
  diagnostic,
  environmentSecrets,
  redact,
  stableJson,
} from "../src/diagnostics";

test("process-free runtimes skip environment secrets and retain explicit redaction", () => {
  const descriptor = Object.getOwnPropertyDescriptor(globalThis, "process")!;
  try {
    Object.defineProperty(globalThis, "process", { configurable: true, value: undefined });
    expect(environmentSecrets()).toEqual([]);
    expect(redact("explicit-private-value", ["explicit-private-value"])).toBe("[REDACTED]");
  } finally {
    Object.defineProperty(globalThis, "process", descriptor);
  }
});

test("hook and ordered cleanup errors preserve original causes and cached close rejection", async () => {
  const root = await mkdtemp(join(tmpdir(), "lenso-causes-"));
  const session = new EngineSession(root, "check");
  const setup = new Error("token=setup-secret");
  const first = new Error("password=first-secret");
  const second = { private: "cleanup-secret" };
  const events: string[] = [];
  try {
    const failure = await withEngine(session, async () => {
      await session.setup([
        {
          name: "resource",
          setup(context) {
            context.onCleanup(() => {
              events.push("first");
              throw first;
            });
            context.onCleanup(() => {
              events.push("second");
              throw second;
            });
            throw setup;
          },
        },
      ]);
    }).catch((cause: unknown) => cause);
    expect(failure).toBeInstanceOf(EngineError);
    const combined = (failure as EngineError).cause as AggregateError;
    expect(combined).toBeInstanceOf(AggregateError);
    expect(combined.errors[0].cause).toBe(setup);
    expect(combined.errors[1].cause.errors).toEqual([second, first]);
    expect(events).toEqual(["second", "first"]);
    expect(session.close()).toBe(session.close());
    expect(await session.close().catch((cause: unknown) => cause)).toBe(combined.errors[1]);
    const serialized = stableJson(diagnostic(failure));
    expect(serialized).not.toContain("secret");
    expect(diagnostic(failure).causes?.map((cause) => cause.code)).toEqual([
      "engine-hook-failed",
      "engine-cleanup-failed",
    ]);
    expect(diagnostic(failure).causes?.[1]?.causes?.map((cause) => cause.pluginId)).toEqual([
      "resource",
      "resource",
    ]);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("a primitive execution failure keeps its identity without cleanup failure", async () => {
  const session = new EngineSession(".", "check");
  const thrown = Symbol("private");
  expect(
    await withEngine(session, async () => {
      throw thrown;
    }).catch((cause: unknown) => cause),
  ).toBe(thrown);
});

test("diagnostics safely describe nested and cyclic Error causes", () => {
  const original = new Error("password=private");
  const wrapper = new Error("token=private", { cause: original });
  original.cause = wrapper;
  const failure = new EngineError(
    {
      code: "engine-hook-failed",
      phase: "engine-generation",
      message: "Generation failed.",
    },
    { cause: wrapper },
  );
  expect(stableJson(diagnostic(failure))).not.toContain("private");
  expect(diagnostic(failure).causes?.[0]?.causes).toHaveLength(1);
  expect("exitCode" in failure).toBe(false);
});

test("EngineError diagnostic causes retain their attributed shape when raw causes coexist", () => {
  const cause = new Error("secret=private");
  const failure = new EngineError(
    {
      code: "engine-cleanup-failed",
      phase: "engine-cleanup",
      message: "Cleanup failed.",
      causes: [
        {
          code: "engine-cleanup-failed",
          phase: "engine-cleanup",
          message: "Plugin cleanup failed.",
          pluginId: "owner",
        },
      ],
    },
    { cause: new AggregateError([cause]) },
  );
  expect(failure.cause).toBeInstanceOf(AggregateError);
  expect(diagnostic(failure).causes?.[0]?.pluginId).toBe("owner");
});
