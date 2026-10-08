import { expect, test } from "bun:test";
import { bindConfig, ConfigError, definePluginConfig, type ConfigSource } from "@lenso/core";
import { describePluginConfig } from "../src/configuration";
import { diagnostic } from "../src/diagnostics";

const schema = {
  "~standard": {
    version: 1 as const,
    vendor: "test",
    validate: (value: unknown) => ({ value }),
  },
};

test("static configuration descriptions never read sources and suppress payload annotations", () => {
  let reads = 0;
  const source: ConfigSource = {
    descriptor: {
      id: "deployment",
      kind: "env",
      location: { file: "https://user:password@example.test/config?token=private" },
      fields: [{ path: ["opaque", 0, "key"], env: "DEPLOYMENT_KEY", sensitive: true }],
    },
    async read() {
      reads++;
      throw Error("private response");
    },
  };
  const inputSchema = {
    type: "object",
    properties: {
      opaque: { type: "array", items: { const: "sensitive-array-value" } },
      port: { type: "number", default: 3001, examples: [9999] },
    },
    $defs: { credential: { const: "sensitive-reference-value" } },
    const: { opaque: "sensitive-root-value" },
    dependentSchemas: { port: { const: "sensitive-dependent-value" } },
    patternProperties: { "sensitive-pattern": { const: "sensitive-pattern-value" } },
  };
  const plugin = bindConfig(
    definePluginConfig({ schema, jsonSchema: () => inputSchema }),
    [source],
    { id: "first", setup: (_context, value) => value },
  );
  const result = describePluginConfig(plugin, "lenso.config.ts");
  expect(reads).toBe(0);
  expect(result?.sources[0]?.fields).toEqual([
    { path: ["opaque", 0, "key"], env: "DEPLOYMENT_KEY", sensitive: true },
  ]);
  expect(result?.inputSchema).toEqual({
    type: "object",
    properties: { opaque: { writeOnly: true }, port: { type: "number" } },
  });
  expect(JSON.stringify(result)).not.toContain("sensitive-array-value");
  expect(JSON.stringify(result)).not.toContain("sensitive-reference-value");
  expect(JSON.stringify(result)).not.toContain("sensitive-root-value");
  expect(JSON.stringify(result)).not.toContain("sensitive-dependent-value");
  expect(JSON.stringify(result)).not.toContain("sensitive-pattern-value");
  expect(JSON.stringify(result)).not.toContain("password");
  expect(JSON.stringify(result)).not.toContain("private");
  expect(inputSchema.properties.port.default).toBe(3001);
});

test("static converter is explicit, optional, and does not inspect validator internals", () => {
  const plugin = bindConfig(
    definePluginConfig({ schema }),
    {},
    {
      id: "runtime",
      setup: (_context, value) => value,
    },
  );
  expect(describePluginConfig(plugin, "app.ts")).toMatchObject({
    schemaAvailability: "runtime-validation-only",
    inputSchema: null,
    sources: [{ id: "values", kind: "values", location: { file: "app.ts" } }],
  });
  const failedConverter = bindConfig(
    definePluginConfig({
      schema,
      jsonSchema() {
        throw Error("secret converter error");
      },
    }),
    {},
    { id: "failed", setup: (_context, value) => value },
  );
  expect(describePluginConfig(failedConverter, "app.ts")?.inputSchema).toBeNull();
});

test("static source locations redact credential paths and token-only URL userinfo", () => {
  const sources: ConfigSource[] = [
    {
      descriptor: {
        id: "path",
        kind: "custom",
        location: { file: "/config/token/opaque-private" },
      },
      async read() {
        throw Error("must not read");
      },
    },
    {
      descriptor: {
        id: "url",
        kind: "custom",
        location: { file: "https://opaque-private@example.test/config" },
      },
      async read() {
        throw Error("must not read");
      },
    },
  ];
  const plugin = bindConfig(definePluginConfig({ schema }), sources, {
    id: "safe-locations",
    setup: (_context, value) => value,
  });
  expect(JSON.stringify(describePluginConfig(plugin, "app.ts"))).not.toContain("opaque-private");
});

test("configuration diagnostics preserve instance, field and source but no raw error causes", () => {
  const error = new ConfigError([
    { code: "config-env-invalid", pluginId: "second", path: ["port"], sourceId: "deployment" },
  ]);
  expect(diagnostic(error, { phase: "setup" })).toMatchObject({
    code: "config-invalid",
    phase: "config",
    causes: [
      {
        code: "config-env-invalid",
        phase: "config",
        pluginId: "second",
        details: { path: ["port"], sourceId: "deployment" },
      },
    ],
  });
  expect(error.cause).toBeUndefined();
});
