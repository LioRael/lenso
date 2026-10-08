import { expect, test } from "bun:test";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { EngineSession, withEngine } from "../src/engine-host";
import { EngineError, diagnostic, stableJson } from "../src/diagnostics";

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

test("diagnostics bound explicit trees and omit unknown detail objects", () => {
  const detail = {
    code: "custom",
    phase: "invoke",
    message: "Safe",
    details: { private: "secret" },
    causes: [] as any[],
  };
  detail.causes.push(detail);
  expect(diagnostic(new EngineError(detail)).details).toBeUndefined();
  expect(stableJson(diagnostic(new EngineError(detail)))).not.toContain("secret");
  const wide = new AggregateError(Array.from({ length: 100 }, () => new Error("secret")));
  expect(diagnostic(wide).causes).toHaveLength(32);
  let deep: Error = new Error("secret");
  for (let i = 0; i < 1000; i++) deep = new Error("secret", { cause: deep });
  expect(stableJson(diagnostic(deep)).length).toBeLessThan(4096);
});

test("trusted adapter wrappers do not repeat the same projected failure", () => {
  const domain = new EngineError(
    { code: "custom-domain", phase: "invoke", message: "Safe" },
    { cause: new Error("private") },
  );
  const wrapper = new EngineError(diagnostic(domain), { cause: domain });
  expect(wrapper.cause).toBe(domain);
  expect(diagnostic(wrapper)).toEqual(diagnostic(domain));
});

test("matching codes do not hide a distinct phase or plugin cause", () => {
  const setup = new EngineError({
    code: "engine-hook-failed",
    phase: "engine-setup",
    pluginId: "database",
    message: "Engine setup failed.",
  });
  const build = new EngineError(
    {
      code: "engine-hook-failed",
      phase: "engine-build",
      pluginId: "builder",
      message: "Build failed.",
    },
    { cause: setup },
  );
  expect(diagnostic(build).causes?.[0]).toMatchObject({
    code: "engine-hook-failed",
    phase: "engine-setup",
    pluginId: "database",
  });
});

test("a config-like custom code cannot opt into configuration details", () => {
  const failure = new EngineError({
    code: "config-unknown-domain",
    phase: "config",
    message: "Safe",
    details: { path: ["PRIVATE-dynamic"], sourceId: "PRIVATE-dynamic" },
  });
  expect(diagnostic(failure).details).toBeUndefined();
  expect(stableJson(diagnostic(failure))).not.toContain("PRIVATE-dynamic");
});
