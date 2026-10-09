import { describe, expect, test } from "bun:test";
import type { StandardSchemaV1 } from "@standard-schema/spec";
import {
  bindConfig,
  ConfigError,
  ConfigSourceError,
  definePluginConfig,
  preflightConfigs,
  resolveConfig,
  valuesSource,
} from "../src/config";
import type { ConfigBinding, ConfigSource, ConfigState } from "../src/config-types";
import { startApp } from "../src/lifecycle";
import { definePlugin } from "../src/plugin";

function schema<Input, Output>(
  validate: StandardSchemaV1<Input, Output>["~standard"]["validate"],
): StandardSchemaV1<Input, Output> {
  return { "~standard": { version: 1, vendor: "test", validate } };
}
const objectContract = definePluginConfig({
  schema: schema<Record<string, unknown>, Record<string, unknown>>((value) => ({
    value: value as Record<string, unknown>,
  })),
});
function binding(sources: readonly ConfigSource[]): ConfigBinding<typeof objectContract.schema> {
  return { contract: objectContract, sources };
}
async function failure(action: () => Promise<unknown>): Promise<ConfigError> {
  try {
    await action();
  } catch (error) {
    expect(error).toBeInstanceOf(ConfigError);
    return error as ConfigError;
  }
  throw new Error("Expected configuration failure.");
}

describe("plugin configuration", () => {
  test("configuration status captures safe preflight metadata and enforces exact dependency access", async () => {
    let reads = 0;
    let secret = "startup-secret";
    const descriptor = {
      id: "remote",
      kind: "custom",
      location: { file: "/private/config.json" },
      fields: [{ path: ["token"], sensitive: true, env: "PRIVATE_TOKEN" }],
    };
    const source: ConfigSource = {
      descriptor,
      async read() {
        reads++;
        return { values: { token: secret }, revision: { token: "private-revision" } };
      },
    };
    const contract = definePluginConfig({
      schema: schema<Record<string, unknown>, Record<string, unknown>>((value) => ({
        value: { ...(value as Record<string, unknown>), derived: true },
      })),
    });
    const first = bindConfig(contract, [source], {
      id: "first",
      setup: (context): ConfigState => context.configuration!(first),
    });
    const second = bindConfig(contract, {}, { id: "second", setup: () => null });
    const observer = definePlugin({
      id: "observer",
      requires: [first],
      setup(context): {
        own: ConfigState;
        read: () => ConfigState;
        denied: () => ConfigState;
        impostor: () => ConfigState;
      } {
        return {
          own: context.configuration!(observer),
          read: () => context.configuration!(first),
          denied: () => context.configuration!(second),
          impostor: () => context.configuration!({ ...first }),
        };
      },
    });
    const app = await startApp({ plugins: [first, second, observer] });
    try {
      const state = app.configuration(first);
      expect(state).toEqual({
        state: "resolved",
        fields: [{ path: ["token"], sourceIds: ["remote"], sensitive: true }],
        sources: [{ id: "remote", kind: "custom" }],
      });
      expect(app.get(first)).toEqual(state);
      expect(app.get(observer).read()).toEqual(state);
      expect(app.get(observer).own).toEqual({
        state: "unconfigured",
        fields: [],
        sources: [],
      });
      expect(app.configuration(second)).toMatchObject({ state: "resolved", fields: [] });
      expect(() => app.configuration({ ...first })).toThrow("not part of this app");
      expect(app.get(observer).denied).toThrow("undeclared configuration");
      expect(app.get(observer).impostor).toThrow("undeclared configuration");
      secret = "changed-secret";
      descriptor.id = "changed";
      descriptor.kind = "changed";
      descriptor.fields[0]!.sensitive = false;
      expect(app.configuration(first)).toEqual(state);
      expect(app.get(observer).read()).toEqual(state);
      expect(reads).toBe(1);
      const json = JSON.stringify(state);
      for (const forbidden of [
        secret,
        "startup-secret",
        "private-revision",
        "/private/config.json",
        "PRIVATE_TOKEN",
        "revision",
        "value",
        "read",
        "derived",
      ])
        expect(json).not.toContain(forbidden);
      for (const frozen of [
        state,
        state.fields,
        state.fields[0],
        state.fields[0]!.path,
        state.fields[0]!.sourceIds,
        state.sources,
        state.sources[0],
      ])
        expect(Object.isFrozen(frozen)).toBe(true);
      expect(Object.isFrozen(descriptor)).toBe(false);
    } finally {
      await app.stop();
    }
  });

  test("plain options, two instances and two starts keep frozen copies isolated", async () => {
    const input = { nested: { count: 1 } };
    const contract = definePluginConfig({
      schema: schema<typeof input, typeof input>((value) => ({ value: value as typeof input })),
    });
    const first = bindConfig(contract, input, { id: "first", setup: (_, config) => config });
    const second = bindConfig(
      contract,
      { nested: { count: 2 } },
      {
        id: "second",
        setup: (_, config) => config,
      },
    );
    const one = await startApp({ plugins: [first, second] });
    const two = await startApp({ plugins: [first, second] });
    expect(one.get(first)).toEqual(input);
    expect(one.get(second).nested.count).toBe(2);
    expect(one.get(first)).not.toBe(two.get(first));
    expect(one.get(first).nested).not.toBe(input.nested);
    expect(Object.isFrozen(one.get(first).nested)).toBe(true);
    expect(Object.isFrozen(input.nested)).toBe(false);
    input.nested.count = 3;
    expect(one.get(first).nested.count).toBe(1);
    await one.stop();
    await two.stop();
  });

  test("all preflight reads happen before legacy or bound setup; validation transforms once", async () => {
    const events: string[] = [];
    let validations = 0;
    const contract = definePluginConfig({
      schema: schema<{ count?: string }, { count: number; derived: boolean }>(async (value) => {
        validations++;
        events.push("validate");
        await Promise.resolve();
        return {
          value: { count: Number((value as { count?: string }).count ?? "7"), derived: true },
        };
      }),
    });
    const source: ConfigSource = {
      descriptor: { id: "async", kind: "custom" },
      async read() {
        events.push("read");
        return { values: {} };
      },
    };
    const legacy = definePlugin({
      id: "legacy",
      setup: () => {
        events.push("legacy");
      },
    });
    const bound = bindConfig(contract, [source], {
      id: "bound",
      setup: (_, config) => {
        events.push("bound");
        return config;
      },
    });
    const app = await startApp({ plugins: [legacy, bound] });
    expect(events).toEqual(["read", "validate", "legacy", "bound"]);
    expect(validations).toBe(1);
    expect(app.get(bound)).toEqual({ count: 7, derived: true });
    const snapshot = await resolveConfig("bound", bound.config!);
    expect(snapshot.provenance).toEqual([]);
    await app.stop();
  });

  test("independent failures aggregate without fallback or any setup", async () => {
    const events: string[] = [];
    const make = (id: string) =>
      bindConfig(
        objectContract,
        [
          {
            descriptor: { id, kind: "custom" },
            async read() {
              events.push(id);
              throw new Error("SECRET");
            },
          },
          {
            descriptor: { id: "fallback", kind: "custom" },
            async read() {
              events.push("fallback");
              return { values: {} };
            },
          },
        ],
        {
          id,
          setup: () => {
            events.push("setup");
          },
        },
      );
    const error = await failure(() => startApp({ plugins: [make("a"), make("b")] }));
    expect(events).toEqual(["a", "b"]);
    expect(error.diagnostics.map((item) => item.pluginId)).toEqual(["a", "b"]);
    expect(JSON.stringify(error)).not.toContain("SECRET");
    expect(error.cause).toBeUndefined();
    expect(error.message).toBe("Plugin configuration failed.");
  });

  test("preflight never sets up; assembly errors win before source reads", async () => {
    let reads = 0;
    const plugin = bindConfig(
      objectContract,
      [
        {
          descriptor: { id: "custom", kind: "custom" },
          async read() {
            reads++;
            return { values: {} };
          },
        },
      ],
      {
        id: "one",
        setup: () => {
          throw new Error("must not run");
        },
      },
    );
    expect((await preflightConfigs([plugin])).has(plugin)).toBe(true);
    expect(reads).toBe(1);
    await expect(startApp({ plugins: [plugin, plugin] })).rejects.toThrow("Duplicate");
    expect(reads).toBe(1);
  });

  test("cancellation is checked without config and after async read and validation", async () => {
    const controller = new AbortController();
    controller.abort("SECRET");
    let setups = 0;
    const legacy = definePlugin({
      id: "legacy",
      setup: () => {
        setups++;
      },
    });
    expect(
      (await failure(() => startApp({ plugins: [legacy] }, controller))).diagnostics[0].code,
    ).toBe("config-cancelled");
    for (const phase of ["read", "validate"]) {
      const abort = new AbortController();
      const contract = definePluginConfig({
        schema: schema<Record<string, unknown>, Record<string, unknown>>(async () => {
          if (phase === "validate") abort.abort("SECRET");
          return { value: {} };
        }),
      });
      const plugin = bindConfig(
        contract,
        [
          {
            descriptor: { id: "custom", kind: "custom" },
            async read(context) {
              expect(context.signal).toBe(abort.signal);
              if (phase === "read") abort.abort("SECRET");
              return { values: {} };
            },
          },
        ],
        {
          id: phase,
          setup: () => {
            setups++;
          },
        },
      );
      const error = await failure(() => startApp({ plugins: [plugin] }, { signal: abort.signal }));
      expect(error.diagnostics[0].code).toBe("config-cancelled");
      expect(JSON.stringify(error)).not.toContain("SECRET");
    }
    expect(setups).toBe(0);
  });

  test("source order replaces whole fields, omits undefined, retains null and sensitivity history", async () => {
    const contract = { ...objectContract, fields: [{ path: ["contract"], sensitive: true }] };
    const result = await resolveConfig("plugin", {
      contract,
      sources: [
        valuesSource(
          { nested: { a: 1 }, array: [1, 2], nullable: 1, omitted: 2, secret: "old", contract: 1 },
          { id: "first", sensitive: [["secret", "child"]] },
        ),
        valuesSource(
          { nested: { b: 2 }, array: [3], nullable: null, omitted: undefined, secret: "new" },
          { id: "second" },
        ),
      ],
    });
    expect(result.value).toEqual({
      nested: { b: 2 },
      array: [3],
      nullable: null,
      omitted: 2,
      secret: "new",
      contract: 1,
    });
    expect(result.provenance.find((field) => field.path[0] === "secret")).toEqual({
      path: ["secret"],
      sourceIds: ["first", "second"],
      sensitive: true,
    });
    expect(result.provenance.find((field) => field.path[0] === "contract")?.sensitive).toBe(true);
    expect(result.provenance.find((field) => field.path[0] === "omitted")?.sourceIds).toEqual([
      "first",
    ]);
  });

  test("root sensitivity covers every raw field even when marked by a later empty source", async () => {
    const result = await resolveConfig(
      "plugin",
      binding([
        valuesSource({ first: 1, second: 2 }),
        valuesSource({}, { id: "sensitivity", sensitive: [[]] }),
      ]),
    );
    expect(result.provenance.every((field) => field.sensitive)).toBe(true);
  });

  test("non-source arrays cannot be interpreted as plain options", async () => {
    const plugin = bindConfig(objectContract, [1, 2] as unknown as readonly ConfigSource[], {
      id: "invalid",
      setup: () => {
        throw new Error("must not setup");
      },
    });
    const error = await failure(() => startApp({ plugins: [plugin] }));
    expect(error.diagnostics[0].code).toBe("config-invalid-data");
  });

  test("schema errors attribute only present raw input fields, not derived or missing fields", async () => {
    const contract = definePluginConfig({
      fields: [{ path: ["port"] }, { path: ["derived"] }, { path: ["missing"] }],
      schema: schema<Record<string, unknown>, Record<string, unknown>>(() => ({
        issues: [
          { path: ["port"], message: "private rejected value" },
          { path: ["derived"], message: "private derived value" },
          { path: ["missing"], message: "private missing value" },
        ],
      })),
    });
    const error = await failure(() =>
      resolveConfig("instance", {
        contract,
        sources: [
          valuesSource({ port: 1 }, { id: "base" }),
          valuesSource({ port: -1 }, { id: "deployment", location: { file: "app.ts" } }),
        ],
      }),
    );
    expect(error.diagnostics).toEqual([
      {
        code: "config-invalid",
        pluginId: "instance",
        path: ["port"],
        sourceId: "deployment",
        source: { file: "app.ts" },
      },
      { code: "config-invalid", pluginId: "instance", path: ["derived"] },
      { code: "config-invalid", pluginId: "instance", path: ["missing"] },
    ]);
    expect(JSON.stringify(error)).not.toContain("private");
  });

  test("source IDs must be unique within an instance to keep attribution unambiguous", async () => {
    const error = await failure(() =>
      resolveConfig("instance", binding([valuesSource({ port: 1 }), valuesSource({ port: 2 })])),
    );
    expect(error.diagnostics[0]).toMatchObject({
      code: "config-invalid-data",
      pluginId: "instance",
      sourceId: "values",
    });
  });

  test("redacted diagnostic keys never collide with raw-input attribution keys", async () => {
    const contract = definePluginConfig({
      fields: [{ path: ["enabled?"] }],
      schema: schema<Record<string, unknown>, Record<string, unknown>>(() => ({
        issues: [{ path: [{ key: "enabled?" }], message: "private error" }],
      })),
    });
    const error = await failure(() =>
      resolveConfig("instance", {
        contract,
        sources: [
          valuesSource({ "enabled?": 1 }, { id: "base", location: { file: "base.ts" } }),
          valuesSource({ "[redacted]": 2 }, { id: "other", location: { file: "other.ts" } }),
        ],
      }),
    );
    expect(error.diagnostics[0]).toEqual({
      code: "config-invalid",
      pluginId: "instance",
      path: ["[redacted]"],
      sourceId: "base",
      source: { file: "base.ts" },
    });
  });

  test("async sources carry opaque revisions without freezing or inspecting them", async () => {
    const revision = { token: "SECRET", resource: () => undefined };
    const source: ConfigSource = {
      descriptor: { id: "custom", kind: "custom" },
      async read() {
        await Promise.resolve();
        return { values: { count: 1 }, revision };
      },
    };
    const result = await resolveConfig("plugin", binding([source]));
    expect(result.revisions[0].revision).toBe(revision);
    expect(Object.isFrozen(revision)).toBe(false);
    expect(result.value).toEqual({ count: 1 });
  });

  test("read context forwards only declared capabilities without freezing caller-owned objects", async () => {
    const context = { signal: new AbortController().signal, credentials: { private: "secret" } };
    await resolveConfig(
      "plugin",
      binding([
        {
          descriptor: { id: "limited", kind: "custom" },
          async read(received) {
            expect(Object.keys(received)).toEqual(["signal"]);
            expect(received.signal).toBe(context.signal);
            expect(received).not.toBe(context);
            expect(Object.isFrozen(received)).toBe(true);
            return { values: {} };
          },
        },
      ]),
      context,
    );
    expect(Object.isFrozen(context)).toBe(false);
    expect(Object.isFrozen(context.credentials)).toBe(false);
  });

  test("dangerous data and resources reject without running getters or freezing original objects", async () => {
    let accessed = 0;
    const accessor = Object.defineProperty({}, "password", {
      enumerable: true,
      get() {
        accessed++;
        throw new Error("SECRET");
      },
    });
    const cycle: Record<string, unknown> = {};
    cycle.self = cycle;
    const bad = [
      JSON.parse('{"nested":{"__proto__":{"polluted":true}}}'),
      { nested: { constructor: "SECRET" } },
      accessor,
      { accessor },
      cycle,
      { value: Infinity },
      { value: () => undefined },
      { value: new Date() },
      { value: Symbol("SECRET") },
      { value: [undefined] },
      Object.create({ inherited: 1 }),
      null,
      1,
    ];
    for (const data of bad) {
      const error = await failure(() =>
        resolveConfig("plugin", binding([valuesSource(data as Record<string, unknown>)])),
      );
      expect(error.diagnostics[0].code).toBe("config-invalid-data");
      expect(JSON.stringify(error)).not.toContain("SECRET");
      if (typeof data === "object" && data !== null) expect(Object.isFrozen(data)).toBe(false);
    }
    expect(accessed).toBe(0);
    expect(Object.prototype).not.toHaveProperty("polluted");
  });

  test("validated output is independently copied and subject to the same plain-data rule", async () => {
    const output = { nested: { count: 1 } };
    const contract = definePluginConfig({
      schema: schema<Record<string, unknown>, typeof output>(() => ({ value: output })),
    });
    const result = await resolveConfig("plugin", { contract, sources: [] });
    expect(result.value).not.toBe(output);
    expect(Object.isFrozen(output)).toBe(false);
    const invalid = definePluginConfig({
      schema: schema<Record<string, unknown>, Record<string, unknown>>(() => ({
        value: { resource: new Date() },
      })),
    });
    expect(
      (await failure(() => resolveConfig("plugin", { contract: invalid, sources: [] })))
        .diagnostics[0].code,
    ).toBe("config-invalid-data");
  });

  test("context only accepts the exact instance binding", async () => {
    const first = bindConfig(objectContract, {}, { id: "first", setup: (_, config) => config });
    const second = bindConfig(
      objectContract,
      {},
      {
        id: "second",
        setup(context) {
          return context.config(first.config!);
        },
      },
    );
    await expect(startApp({ plugins: [first, second] })).rejects.toThrow(
      "undeclared configuration",
    );
  });

  test("legacy contexts stay valid for plain plugins but cannot bypass configured preflight", () => {
    const context = {
      instanceId: "legacy",
      get() {
        throw new Error("No dependencies");
      },
      onCleanup: () => async () => {},
    };
    const plain = definePlugin({ id: "plain", setup: () => "legacy service" });
    expect(plain.setup(context)).toBe("legacy service");
    let setups = 0;
    const bound = bindConfig(
      objectContract,
      {},
      {
        id: "bound",
        setup() {
          setups++;
        },
      },
    );
    expect(() => bound.setup(context)).toThrow(ConfigError);
    expect(setups).toBe(0);
  });

  test("schema issues and adapter errors retain only safe paths and scrub credential locations", async () => {
    const contract = definePluginConfig({
      fields: [{ path: ["nested", 0, "field"] }],
      schema: schema<Record<string, unknown>, Record<string, unknown>>(() => ({
        issues: [
          {
            message: "SECRET",
            path: [{ key: "nested" }, { key: 0 }, { key: "field" }],
          },
          { message: "SECRET", path: [{ key: Symbol("SECRET") }] },
        ],
      })),
    });
    const error = await failure(() => resolveConfig("plugin", { contract, sources: [] }));
    expect(error.diagnostics.map((item) => item.path)).toEqual([["nested", 0, "field"], []]);
    expect(JSON.stringify(error)).not.toContain("SECRET");
    const sourceError = new ConfigSourceError("config-file-missing", ["file"]);
    const source: ConfigSource = {
      descriptor: {
        id: "file",
        kind: "file",
        fields: [{ path: ["file"] }],
        location: { file: "https://user:SECRET@example.com/config?token=SECRET" },
      },
      async read() {
        throw sourceError;
      },
    };
    const failureError = await failure(() => resolveConfig("plugin", binding([source])));
    expect(failureError.diagnostics[0]).toEqual({
      code: "config-file-missing",
      pluginId: "plugin",
      sourceId: "file",
      path: ["file"],
      source: { file: "[redacted]" },
    });
    expect(sourceError.cause).toBeUndefined();
    expect(sourceError.message).toBe("Configuration source failed.");
  });

  test("dynamic configuration keys never become public diagnostic paths", async () => {
    const marker = "PRIVATE-record-key";
    const contract = definePluginConfig({
      jsonSchema: () => ({
        type: "object",
        properties: { labels: { type: "object", additionalProperties: { type: "number" } } },
      }),
      schema: schema<Record<string, unknown>, Record<string, unknown>>(() => ({
        issues: [
          { path: ["labels", marker], message: marker },
          { path: [marker], message: marker },
        ],
      })),
    });
    const error = await failure(() =>
      resolveConfig("plugin", {
        contract,
        sources: [valuesSource({ labels: { [marker]: "bad" } })],
      }),
    );
    expect(error.diagnostics.map((item) => item.path)).toEqual([["labels"], []]);
    expect(JSON.stringify(error)).not.toContain(marker);
    expect(error.cause).toBeUndefined();
  });

  test("schema-thrown errors are replaced even if they masquerade as ConfigError", async () => {
    const original = new ConfigError([{ code: "config-invalid", pluginId: "not-the-plugin" }]);
    original.message = "SECRET";
    original.cause = "SECRET";
    const contract = definePluginConfig({
      schema: schema<Record<string, unknown>, Record<string, unknown>>(() => {
        throw original;
      }),
    });
    const error = await failure(() => resolveConfig("plugin", { contract, sources: [] }));
    expect(error).not.toBe(original);
    expect(error.message).toBe("Plugin configuration failed.");
    expect(error.cause).toBeUndefined();
    expect(error.diagnostics).toEqual([{ code: "config-invalid", pluginId: "plugin" }]);
  });
});
