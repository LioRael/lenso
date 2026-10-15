import { expect, spyOn, test } from "bun:test";
import { definePlugin, type RunningApp } from "@lenso/core";
import { BusinessError, RuntimeHost, type HandlerContext } from "@lenso/runtime-sdk";
import { validateContract, validateValue } from "@lenso/runtime-sdk/schema";
import type { StandardSchemaV1 } from "@standard-schema/spec";
import {
  defineRuntimeService,
  exportRuntimeContract,
  runtimeSchema,
  RuntimeSchemaError,
  type InferRuntimeSchema,
  type RuntimeSchema,
  type RuntimeSchemaDefinition,
} from "../src/authoring";
import { createRuntimePlugin, startRuntimeApp } from "../src/index";

const input = runtimeSchema({
  type: "object",
  properties: {
    text: { type: "string", minLength: 1, maxLength: 10 },
    count: { type: "integer", minimum: 1, maximum: 3 },
    tags: { type: "array", items: { type: "string", enum: ["a", "b"] }, maxItems: 2 },
    note: { anyOf: [{ type: "string" }, { type: "null" }] },
  },
  required: ["text", "tags", "note"],
  additionalProperties: false,
});
const output = runtimeSchema({ type: "string", minLength: 1 });
const errorSchema = runtimeSchema({
  type: "object",
  properties: { code: { type: "string", enum: ["missing", "busy"] } },
  required: ["code"],
  additionalProperties: false,
});
const service = defineRuntimeService({
  version: "1",
  methods: {
    greet: { mode: "unary", input, output, error: errorSchema },
    watch: { mode: "stream", input, output, error: errorSchema },
  },
});

// These compile checks guard the public API, including schema literal inference.
function typeChecks(context: HandlerContext) {
  type Input = StandardSchemaV1.InferOutput<typeof input>;
  const valid: Input = { text: "hello", tags: ["a"], note: null };
  valid.text = "changed"; // Schema literal readonly modifiers must not infect business data.
  valid.count = 2;
  // @ts-expect-error Required nullable means present, not optional.
  const missing: Input = { text: "hello", tags: [] };
  // @ts-expect-error Literal enum members remain narrow.
  const badEnum: Input = { text: "hello", tags: ["c"], note: null };
  // @ts-expect-error A required field is not optional.
  const wrong: Input = { tags: [], note: null };
  service.bind(() => ({
    // @ts-expect-error The input parameter is inferred from the schema.
    greet: (value: { text: number }) => String(value.text),
    watch: async function* () {
      yield "event";
    },
  }));
  service.bind(() => ({
    // @ts-expect-error Unary return values must match the output schema.
    greet: () => 4,
    // @ts-expect-error Streams must be AsyncIterable, not an ordinary Promise of an item.
    watch: async () => "event",
  }));
  // @ts-expect-error Business error details are inferred from the method's error schema.
  service.businessError("greet", { code: "unknown" });
  type Widened = InferRuntimeSchema<{
    type: "object";
    properties: { name: { type: "string" } };
    required: string[];
  }>;
  const widened: Widened = { name: "value" };
  // @ts-expect-error Widened schemas produce unknown, requiring explicit narrowing.
  const unsafeName: string = widened.name;
  void unsafeName;
  const broad = runtimeSchema({
    type: "object",
    properties: { name: { type: "string" } },
    required: ["name"] as string[],
    additionalProperties: false,
  });
  defineRuntimeService({
    version: "1",
    methods: {
      greet: { mode: "unary", input: broad, output, error: errorSchema },
    },
  }).bind(() => ({
    // @ts-expect-error Widening must never allow unrelated narrow handler inputs.
    greet: (value: number) => value.toFixed(2),
  }));
  defineRuntimeService({
    version: "1",
    methods: {
      greet: { mode: "unary" as "unary" | "stream", input, output, error: errorSchema },
    },
  }).bind(() => ({
    // @ts-expect-error A widened mode cannot promise a unary result.
    greet: () => "value",
  }));
  const factory = service.bind(() => ({
    greet: (value, invocation) => `${value.text}:${invocation.callId}`,
    watch: async function* (value) {
      yield value.text;
    },
  }));
  void [missing, badEnum, wrong, widened, factory, context];
}
void typeChecks;

test("one explicit schema preserves v1 constraints and validates native input", async () => {
  expect(await input["~standard"].validate({ text: "hi", tags: ["a"], note: null })).toEqual({
    value: { text: "hi", tags: ["a"], note: null },
  });
  for (const value of [
    { text: "", tags: [], note: null },
    { text: "hi", tags: ["c"], note: null },
    { text: "hi", tags: [], count: 4, note: null },
    { text: "hi", tags: [] },
    { text: "hi", tags: [], note: null, secret: "extra" },
  ])
    expect(await input["~standard"].validate(value)).toHaveProperty("issues");
  validateContract(service.v1Contract);
  expect(service.v1Contract.methods.greet!.input.properties!.text).toEqual({
    type: "string",
    minLength: 1,
    maxLength: 10,
  });
  expect(service.businessError("greet", { code: "busy" })).toBeInstanceOf(BusinessError);
});

test("Standard JSON Schema conversion retains implicit safe-integer bounds without changing wire data", () => {
  const schema = runtimeSchema({
    type: "array",
    items: { anyOf: [{ type: "integer", minimum: 0 }, { type: "null" }] },
  });
  const json = schema["~standard"].jsonSchema.output({ target: "draft-2020-12" });
  expect(json).toEqual({
    type: "array",
    items: {
      anyOf: [{ type: "integer", minimum: 0, maximum: Number.MAX_SAFE_INTEGER }, { type: "null" }],
    },
  });
  expect(input["~standard"].jsonSchema.input({ target: "draft-07" })).toEqual(
    service.v1Contract.methods.greet!.input,
  );
  expect(() => schema["~standard"].jsonSchema.input({ target: "openapi-3.0" })).toThrow();
  const contract = defineRuntimeService({
    version: "1",
    methods: {
      read: { mode: "unary", input: schema, output, error: errorSchema },
    },
  }).v1Contract;
  expect(contract.methods.read!.input.items!.anyOf![0]).toEqual({ type: "integer", minimum: 0 });
});

test("unsupported constraints fail with nested declaration paths without conversion", () => {
  for (const [schema, path] of [
    [
      {
        type: "object",
        properties: { text: { type: "string", pattern: "^x$" } },
        required: ["text"],
        additionalProperties: false,
      },
      ["properties", "text", "pattern"],
    ],
    [{ type: "array", items: { type: "number", multipleOf: 2 } }, ["items", "multipleOf"]],
    [{ anyOf: [{ type: "string", format: "email" }, { type: "null" }] }, ["anyOf", 0, "format"]],
    [{ type: "string", default: "x" }, ["default"]],
    [{ type: "string", pattern: undefined }, ["pattern"]],
    [
      {
        type: "object",
        properties: { note: { anyOf: [{ type: "string" }, { type: "null" }] } },
        required: [],
        additionalProperties: false,
      },
      ["properties", "note"],
    ],
  ] as const) {
    let failure: unknown;
    try {
      runtimeSchema(schema as unknown as RuntimeSchemaDefinition);
    } catch (error) {
      failure = error;
    }
    expect(failure).toBeInstanceOf(RuntimeSchemaError);
    expect((failure as RuntimeSchemaError).path).toEqual(path);
  }
});

test("invalid schemas and lossy JSON values are refused before export", () => {
  const cyclic: Record<string, unknown> = { type: "array" };
  cyclic.items = cyclic;
  for (const schema of [
    { type: "number", minimum: 3, maximum: 2 },
    { type: "number", maximum: Infinity },
    { type: "string", enum: [] },
    { type: "string", enum: ["a", "a"] },
    { type: "string", enum: Array.from({ length: 65 }, (_, i) => String(i)) },
    { type: "string", minLength: -1 },
    { type: "string", transform: () => "x" },
    { type: "string", [Symbol("hidden")]: true },
    { type: "object", properties: {}, required: [], additionalProperties: true },
    { anyOf: [{ type: "string" }, { type: "number" }] },
    new Date(),
    cyclic,
  ])
    expect(() => runtimeSchema(schema as RuntimeSchemaDefinition)).toThrow(RuntimeSchemaError);
  let deep: RuntimeSchemaDefinition = { type: "string" };
  for (let i = 0; i < 66; i++) deep = { type: "array", items: deep };
  expect(() => runtimeSchema(deep)).toThrow(RuntimeSchemaError);
  expect(() =>
    defineRuntimeService({
      version: "1",
      methods: {
        invalid: {
          mode: "unary",
          input: { "~standard": input["~standard"] } as RuntimeSchema,
          output,
          error: errorSchema,
        },
      },
    }),
  ).toThrow("#/methods/invalid/input");
  let reads = 0;
  const values = Object.defineProperty(["a"], "0", {
    enumerable: true,
    get() {
      reads++;
      return "a";
    },
  });
  expect(() => runtimeSchema({ type: "string", enum: values })).toThrow("#/enum/0");
  expect(reads).toBe(0);
  const method = { mode: "unary" as const, input, output, error: errorSchema };
  expect(() =>
    defineRuntimeService({
      version: "1",
      methods: Object.defineProperty({}, "greet", { value: method }),
    }),
  ).toThrow("#/methods/greet");
  expect(() =>
    defineRuntimeService({
      version: "1",
      methods: { greet: { ...method, [Symbol("constraint")]: true } },
    }),
  ).toThrow("#/methods/greet");
  expect(() => defineRuntimeService({ version: "1", methods: [] as never })).toThrow("#/methods");
});

test("raw v1 contracts stay live and are revalidated, without identity caching", () => {
  const contract = exportRuntimeContract(service.v1Contract);
  contract.methods.greet!.input.properties!.text!.maxLength = 1;
  expect(exportRuntimeContract(contract).methods.greet!.input.properties!.text!.maxLength).toBe(1);
  expect(() =>
    validateValue(contract.methods.greet!.input, { text: "hi", tags: [], note: null }),
  ).toThrow();
  (contract.methods.greet!.input.properties!.text as Record<string, unknown>).pattern = "^x$";
  expect(() => exportRuntimeContract(contract)).toThrow(
    "#/methods/greet/input/properties/text/pattern",
  );
});

test("binding selects only declared ordinary functions after all native setup", async () => {
  const events: string[] = [];
  let bound: Record<string, unknown> | undefined;
  const host = {
    activate: async (handlers: Record<string, unknown>) => {
      bound = handlers;
      events.push("ready");
    },
    drain: async () => {
      events.push("drain");
    },
    close: async () => {
      events.push("close");
    },
  } as unknown as RuntimeHost;
  const connect = spyOn(RuntimeHost, "connect").mockResolvedValue(host);
  const ordinary = async (value: StandardSchemaV1.InferOutput<typeof input>) => value.text;
  const stream = async function* () {
    yield "event";
  };
  const exports = service.bind((running) => {
    events.push("bind");
    const native = running.get(provider);
    return { greet: native.greet, watch: native.watch };
  });
  const runtime = createRuntimePlugin({
    id: "runtime",
    connection: { namespace: "test", service: "native", contract: service.v1Contract },
    clients: () => undefined,
    exports,
  });
  const provider = definePlugin({
    id: "native",
    requires: [runtime],
    setup(context) {
      events.push("setup");
      context.onCleanup(() => {
        events.push("cleanup");
      });
      return { greet: ordinary, watch: stream, hidden: () => "private" };
    },
  });
  try {
    expect(events).toEqual([]);
    const app = await startRuntimeApp({ plugins: [runtime, provider] });
    try {
      expect(events).toEqual(["setup", "bind", "ready"]);
      expect(bound).toEqual({ greet: ordinary, watch: stream });
      expect(bound!.greet).toBe(ordinary);
      expect(connect.mock.calls[0]![0].ready).toBe(false);
    } finally {
      await app.stop();
    }
    expect(events).toEqual(["setup", "bind", "ready", "drain", "close", "cleanup"]);
  } finally {
    connect.mockRestore();
  }
});

test("structurally typed extra or missing handler selections fail closed", () => {
  const handlers = {
    greet: () => "ok",
    watch: async function* () {
      yield "ok";
    },
    hidden: () => "private",
  };
  expect(() => service.bind(() => handlers)({} as RunningApp)).toThrow("undeclared method");
  expect(() =>
    service.bind(() => ({ greet: handlers.greet }) as typeof handlers)({} as RunningApp),
  ).toThrow("missing");
});
