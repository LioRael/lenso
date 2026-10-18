import { expect, test } from "bun:test";
import type { StandardSchemaV1 } from "@standard-schema/spec";
import { bindConfig, definePluginConfig } from "../src/config";
import { startApp } from "../src/lifecycle";
import {
  defineInitialization,
  initializationConfigBlockers,
  InitializationError,
  type InitializationRecord,
} from "../src/initialization";

const input: StandardSchemaV1<unknown, { password: string }> = {
  "~standard": {
    version: 1,
    vendor: "fixture",
    validate(value) {
      return value &&
        typeof value === "object" &&
        "password" in value &&
        typeof value.password === "string"
        ? { value: { password: value.password } }
        : { issues: [{ message: "private-validator-text" }] };
    },
  },
};

test("startup reports uninitialized before any config/source/setup; expired authority cannot inspect or claim", async () => {
  let reads = 0,
    sources = 0,
    setups = 0,
    authorized = true;
  const proof = {};
  const initialization = defineInitialization({
    input: definePluginConfig({ schema: input }),
    authorize: (_action, evidence) => authorized && evidence === proof,
    read: async () => {
      reads++;
      return null;
    },
    commit: async () => {
      throw new Error("must not commit");
    },
    check: async () => [],
    complete: async () => {
      throw new Error("must not complete");
    },
  });
  const plugin = bindConfig(
    definePluginConfig({ schema: input }),
    [
      {
        descriptor: { id: "source", kind: "custom" },
        read: async () => {
          sources++;
          return { values: {} };
        },
      },
    ],
    {
      id: "business",
      setup() {
        setups++;
      },
    },
  );
  await expect(startApp({ plugins: [plugin], initialization })).rejects.toMatchObject({
    code: "initialization-required",
  });
  expect([sources, setups]).toEqual([0, 0]);
  const before = reads;
  await expect(
    initialization.apply(
      { requestId: "attempt-123", input: { password: "never-return-this" } },
      { owner: true },
    ),
  ).rejects.toMatchObject({ code: "initialization-unauthorized" });
  authorized = false;
  await expect(initialization.status(proof)).rejects.toMatchObject({
    code: "initialization-unauthorized",
  });
  expect(reads).toBe(before);
});

test("Workers without an external secret writer stay blocked and never commit or echo secret input", async () => {
  let commits = 0;
  const initialization = defineInitialization({
    input: definePluginConfig({ schema: input }),
    authorize: () => true,
    read: async () => null,
    beforeCommit: async () => [
      { code: "external-configuration-required", password: "private-callback-value" } as never,
    ],
    commit: async () => {
      commits++;
      throw new Error("must not commit");
    },
    check: async () => [],
    complete: async () => {
      throw new Error("must not complete");
    },
  });
  const status = await initialization.apply(
    { requestId: "attempt-123", input: { password: "private-input-value" } },
    {},
  );
  expect(status).toMatchObject({
    state: "blocked",
    nextAction: "external_configuration",
    blockers: [{ code: "external-configuration-required" }],
  });
  expect(commits).toBe(0);
  expect(JSON.stringify(status)).not.toContain("private");
});

test("a committed-but-disconnected setup is queried and activated without recreating the owner", async () => {
  let stored: InitializationRecord | null = null;
  let commits = 0,
    failActivation = true;
  const initialization = defineInitialization({
    input: definePluginConfig({ schema: input }),
    authorize: () => true,
    read: async () => stored,
    commit: async ({ requestId }) => {
      commits++;
      stored = { requestId, revision: "revision-1", activated: false };
      throw new Error("private-provider-error-after-commit");
    },
    check: async () => [],
    activate: async () => {
      if (failActivation) throw new Error("private-provider-activation-error");
    },
    complete: async () => (stored = { ...stored!, activated: true }),
  });
  await expect(
    initialization.apply(
      { requestId: "attempt-123", input: { password: "private-input-value" } },
      {},
    ),
  ).rejects.toBeInstanceOf(InitializationError);
  expect(await initialization.status({})).toMatchObject({
    state: "awaiting_activation",
    revision: "revision-1",
  });
  const failed = await initialization.activate({});
  expect(failed).toMatchObject({
    state: "awaiting_activation",
    blockers: [{ code: "activation-failed" }],
  });
  expect(JSON.stringify(failed)).not.toContain("private");
  failActivation = false;
  expect(await initialization.activate({})).toMatchObject({
    state: "ready",
    revision: "revision-1",
  });
  expect(commits).toBe(1);
});

test("initialization preflight reuses configuration validation and safe field diagnostics without setup", async () => {
  const plugin = bindConfig(
    definePluginConfig({ schema: input, fields: [{ path: ["password"], sensitive: true }] }),
    {},
    {
      id: "credentials",
      setup() {
        throw new Error("must not start");
      },
    },
  );
  const blockers = await initializationConfigBlockers([plugin]);
  expect(blockers).toMatchObject([
    {
      code: "configuration-invalid",
      diagnostics: [{ code: "config-invalid", pluginId: "credentials" }],
    },
  ]);
  expect(JSON.stringify(blockers)).not.toContain("private-validator-text");
});

test("staged Worker secrets are not activation: the selected environment and revision must be observed", async () => {
  let bindingPresent = false,
    liveRevision = "old-revision";
  let stored: InitializationRecord = {
    revision: "staged-revision",
    requestId: "attempt-123",
    activated: false,
  };
  const initialization = defineInitialization({
    input: definePluginConfig({ schema: input }),
    authorize: () => true,
    read: async () => stored,
    commit: async () => {
      throw new Error("bootstrap already committed");
    },
    check: async (record) =>
      !bindingPresent
        ? [{ code: "external-configuration-required" }]
        : liveRevision !== record.revision
          ? [{ code: "activation-failed" }]
          : [],
    activate: async (record) => {
      if (liveRevision !== record.revision) throw new Error("staged version is not serving");
    },
    complete: async (revision) => {
      expect(revision).toBe("staged-revision");
      return (stored = { ...stored, activated: true });
    },
  });
  expect(await initialization.activate({})).toMatchObject({
    state: "blocked",
    nextAction: "external_configuration",
  });
  bindingPresent = true;
  expect(await initialization.activate({})).toMatchObject({ state: "awaiting_activation" });
  liveRevision = "staged-revision";
  expect(await initialization.activate({})).toMatchObject({ state: "ready" });
  liveRevision = "old-revision";
  await expect(initialization.assertReady()).rejects.toMatchObject({
    code: "initialization-not-ready",
  });
  liveRevision = "staged-revision";
  await initialization.assertReady();
});

test("apply never bypasses separate activation authorization after its durable commit", async () => {
  let stored: InitializationRecord | null = null,
    allowed = false,
    activations = 0;
  const initialization = defineInitialization({
    input: definePluginConfig({ schema: input }),
    authorize: (action) => action !== "activate" || allowed,
    read: async () => stored,
    commit: async ({ requestId }) =>
      (stored = { requestId, revision: "revision-1", activated: false }),
    check: async () => [],
    activate: async () => {
      activations++;
    },
    complete: async () => (stored = { ...stored!, activated: true }),
  });
  await expect(
    initialization.apply({ requestId: "attempt-123", input: { password: "private" } }, {}),
  ).rejects.toMatchObject({ code: "initialization-unauthorized" });
  expect(activations).toBe(0);
  expect(await initialization.status({})).toMatchObject({ state: "awaiting_activation" });
  allowed = true;
  expect(await initialization.activate({})).toMatchObject({ state: "ready" });
});
