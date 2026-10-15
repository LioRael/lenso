import type { RunningApp } from "@lenso/core";
import type { StandardJSONSchemaV1, StandardSchemaV1 } from "@standard-schema/spec";
import { BusinessError, type HandlerContext } from "@lenso/runtime-sdk";
import {
  type Contract,
  type Schema,
  validateContract,
  validateSchema,
  validateValue,
} from "@lenso/runtime-sdk/schema";
import type { RuntimeHandlers } from "./index";

/** The existing Runtime v1 subset, expressed in TypeScript rather than a JSON source file. */
export interface RuntimeSchemaDefinition {
  readonly type?: Schema["type"];
  readonly properties?: Readonly<Record<string, RuntimeSchemaDefinition>>;
  readonly required?: readonly string[];
  readonly additionalProperties?: false;
  readonly items?: RuntimeSchemaDefinition;
  readonly enum?: readonly (string | number | boolean | null)[];
  readonly minimum?: number;
  readonly maximum?: number;
  readonly minLength?: number;
  readonly maxLength?: number;
  readonly minItems?: number;
  readonly maxItems?: number;
  readonly anyOf?: readonly RuntimeSchemaDefinition[];
}

type Simplify<T> = { -readonly [K in keyof T]: T[K] };
type ObjectValue<P, R extends readonly string[]> = R extends readonly string[]
  ? number extends R["length"]
    ? unknown
    : string extends R[number]
      ? unknown
      : Simplify<
          { [K in keyof P as K extends R[number] ? K : never]: InferRuntimeSchema<P[K]> } & {
            [K in keyof P as K extends R[number] ? never : K]?: InferRuntimeSchema<P[K]>;
          }
        >
  : unknown;
export type InferRuntimeSchema<S> = S extends { anyOf: readonly (infer B)[] }
  ? InferRuntimeSchema<B>
  : S extends { enum: readonly (infer E)[] }
    ? E
    : S extends { type: "string" }
      ? string
      : S extends { type: "boolean" }
        ? boolean
        : S extends { type: "integer" | "number" }
          ? number
          : S extends { type: "null" }
            ? null
            : S extends { type: "array"; items: infer I }
              ? InferRuntimeSchema<I>[]
              : S extends {
                    type: "object";
                    properties: infer P;
                    required: infer R extends readonly string[];
                  }
                ? ObjectValue<P, R>
                : unknown;

const schemaType: unique symbol = Symbol("runtimeSchema");
export interface RuntimeSchema<T = unknown> extends StandardSchemaV1<T, T> {
  readonly [schemaType]: true;
  readonly "~standard": StandardSchemaV1.Props<T, T> & StandardJSONSchemaV1.Props<T, T>;
}
const definitions = new WeakMap<object, Schema>();

export class RuntimeSchemaError extends Error {
  readonly path: readonly (string | number)[];
  constructor(path: readonly (string | number)[], reason: string) {
    const pointer = path.map((part) => String(part).replaceAll("~", "~0").replaceAll("/", "~1"));
    super(`Runtime v1 schema at #${pointer.length ? `/${pointer.join("/")}` : ""}: ${reason}`);
    this.name = "RuntimeSchemaError";
    this.path = [...path];
  }
}

function dataObject(
  value: unknown,
  path: (string | number)[],
  allowed?: readonly string[],
): asserts value is Record<string, unknown> {
  if (
    !value ||
    typeof value !== "object" ||
    Array.isArray(value) ||
    ![Object.prototype, null].includes(Object.getPrototypeOf(value))
  )
    throw new RuntimeSchemaError(path, "expected a plain object");
  for (const key of Reflect.ownKeys(value)) {
    if (typeof key !== "string")
      throw new RuntimeSchemaError(path, "symbol keys cannot be represented");
    const property = Object.getOwnPropertyDescriptor(value, key)!;
    if (!property.enumerable || !("value" in property))
      throw new RuntimeSchemaError([...path, key], "expected an enumerable data property");
    if (allowed && !allowed.includes(key))
      throw new RuntimeSchemaError([...path, key], "unsupported declaration field");
  }
}

// Copy only lossless JSON data. JSON.stringify alone would silently erase undefined,
// functions, symbols and non-enumerable properties before the SDK could reject them.
function copyJson(
  value: unknown,
  path: (string | number)[] = [],
  seen = new Set<object>(),
): unknown {
  const fail = (reason: string): never => {
    throw new RuntimeSchemaError(path, reason);
  };
  if (value === null || typeof value === "string" || typeof value === "boolean") return value;
  if (typeof value === "number" && Number.isFinite(value)) return value;
  if (typeof value !== "object" || value === null) return fail("expected finite JSON data");
  if (path.length > 256 || seen.has(value)) return fail("cyclic or excessively deep definition");
  if (!Array.isArray(value) && ![Object.prototype, null].includes(Object.getPrototypeOf(value)))
    return fail("expected a plain JSON object");
  seen.add(value);
  try {
    if (Array.isArray(value)) {
      if (Reflect.ownKeys(value).length !== value.length + 1)
        return fail("arrays must contain only dense indexed values");
      return Array.from({ length: value.length }, (_, index) => {
        const property = Object.getOwnPropertyDescriptor(value, String(index));
        if (!property?.enumerable || !("value" in property))
          throw new RuntimeSchemaError([...path, index], "expected an enumerable data property");
        return copyJson(property.value, [...path, index], seen);
      });
    }
    const result: Record<string, unknown> = Object.create(null);
    for (const key of Reflect.ownKeys(value)) {
      if (typeof key !== "string") return fail("symbol keys cannot be represented");
      const property = Object.getOwnPropertyDescriptor(value, key)!;
      if (!property.enumerable || !("value" in property))
        throw new RuntimeSchemaError([...path, key], "expected an enumerable data property");
      result[key] = copyJson(property.value, [...path, key], seen);
    }
    return result;
  } finally {
    seen.delete(value);
  }
}

/** Localize SDK validation failures without converting or removing constraints. */
function checkSchema(
  schema: unknown,
  path: (string | number)[],
  depth = 0,
): asserts schema is Schema {
  if (depth > 64) throw new RuntimeSchemaError(path, "schema too deep");
  if (schema && typeof schema === "object" && !Array.isArray(schema)) {
    const node = schema as Schema;
    if (node.properties && typeof node.properties === "object" && !Array.isArray(node.properties)) {
      for (const [key, child] of Object.entries(node.properties)) {
        const childPath = [...path, "properties", key];
        checkSchema(child, childPath, depth + 1);
        if (
          Array.isArray(node.required) &&
          !node.required.includes(key) &&
          (child.anyOf || child.type === "null")
        )
          throw new RuntimeSchemaError(childPath, "optional nullable fields are unsupported");
      }
    }
    if (node.items !== undefined) checkSchema(node.items, [...path, "items"], depth + 1);
    if (Array.isArray(node.anyOf))
      node.anyOf.forEach((child, index) =>
        checkSchema(child, [...path, "anyOf", index], depth + 1),
      );
  }
  try {
    validateSchema(schema as Schema);
  } catch (error) {
    const reason = error instanceof Error ? error.message : "invalid schema";
    const keyword = reason.startsWith("unsupported schema keyword: ")
      ? reason.slice("unsupported schema keyword: ".length)
      : /^keyword (.*) incompatible with /.exec(reason)?.[1];
    throw new RuntimeSchemaError(keyword ? [...path, keyword] : path, reason);
  }
}

export function runtimeSchema<const S extends RuntimeSchemaDefinition>(
  definition: S,
): RuntimeSchema<InferRuntimeSchema<S>> {
  const schema = copyJson(definition);
  checkSchema(schema, []);
  const jsonSchema = ({ target }: StandardJSONSchemaV1.Options): Record<string, unknown> => {
    if (target !== "draft-2020-12" && target !== "draft-07")
      throw new Error("Runtime schema supports draft-2020-12 and draft-07 JSON Schema targets.");
    const result = structuredClone(schema);
    const constrainIntegers = (node: Schema) => {
      // Runtime's integer type means a JS safe integer, an implicit constraint
      // that ordinary JSON Schema validators need stated explicitly.
      if (node.type === "integer") {
        node.minimum = Math.max(node.minimum ?? Number.MIN_SAFE_INTEGER, Number.MIN_SAFE_INTEGER);
        node.maximum = Math.min(node.maximum ?? Number.MAX_SAFE_INTEGER, Number.MAX_SAFE_INTEGER);
      }
      for (const child of Object.values(node.properties ?? {})) constrainIntegers(child);
      if (node.items) constrainIntegers(node.items);
      node.anyOf?.forEach(constrainIntegers);
    };
    constrainIntegers(result);
    return result;
  };
  const result: RuntimeSchema<InferRuntimeSchema<S>> = {
    [schemaType]: true,
    "~standard": {
      version: 1,
      vendor: "@lenso/runtime",
      jsonSchema: { input: jsonSchema, output: jsonSchema },
      validate(value) {
        try {
          validateValue(schema, value);
          return { value: value as InferRuntimeSchema<S> };
        } catch {
          return { issues: [{ message: "Value does not satisfy the Runtime schema." }] };
        }
      },
    },
  };
  definitions.set(result, schema);
  return result;
}

export interface RuntimeMethod {
  readonly mode: "unary" | "stream";
  readonly input: RuntimeSchema;
  /** For stream methods this describes each yielded item. */
  readonly output: RuntimeSchema;
  readonly error: RuntimeSchema;
}
type Value<S extends RuntimeSchema> = StandardSchemaV1.InferOutput<S>;
export type RuntimeServiceHandlers<M extends Record<string, RuntimeMethod>> = {
  [K in keyof M]: (
    input: Value<M[K]["input"]>,
    context: HandlerContext,
  ) => M[K]["mode"] extends "unary"
    ? Value<M[K]["output"]> | Promise<Value<M[K]["output"]>>
    : M[K]["mode"] extends "stream"
      ? AsyncIterable<Value<M[K]["output"]>> | Promise<AsyncIterable<Value<M[K]["output"]>>>
      : never;
};

/** Validate raw exported v1 contracts too, retaining the declaration path in failures. */
export function exportRuntimeContract(value: unknown): Contract {
  const contract = copyJson(value) as Contract;
  if (
    contract &&
    typeof contract === "object" &&
    contract.methods &&
    typeof contract.methods === "object"
  ) {
    for (const [name, method] of Object.entries(contract.methods)) {
      if (!method || typeof method !== "object") continue;
      for (const part of ["input", "output", "error"] as const)
        checkSchema(method[part], ["methods", name, part]);
    }
  }
  try {
    validateContract(contract);
  } catch (error) {
    throw new RuntimeSchemaError([], error instanceof Error ? error.message : "invalid contract");
  }
  return contract;
}

export function defineRuntimeService<const M extends Record<string, RuntimeMethod>>(definition: {
  readonly version: string;
  readonly methods: M;
}) {
  dataObject(definition, [], ["version", "methods"]);
  dataObject(definition.methods, ["methods"]);
  const methods = Object.fromEntries(
    Object.entries(definition.methods).map(([name, method]) => {
      dataObject(method, ["methods", name], ["mode", "input", "output", "error"]);
      const schemas = Object.fromEntries(
        (["input", "output", "error"] as const).map((part) => {
          const schema = definitions.get(method[part]);
          if (!schema)
            throw new RuntimeSchemaError(
              ["methods", name, part],
              "use runtimeSchema with an explicit v1 definition",
            );
          return [part, schema];
        }),
      );
      return [name, { mode: method.mode, ...schemas }];
    }),
  );
  const contract = exportRuntimeContract({ version: definition.version, methods });
  return {
    /** The ordinary live SDK contract; no freezing or identity-based validation cache. */
    get v1Contract(): Contract {
      return contract;
    },
    /** The factory runs through createRuntimePlugin.exports, after all Core setup. */
    bind(
      factory: (running: RunningApp) => RuntimeServiceHandlers<M>,
    ): (running: RunningApp) => RuntimeHandlers {
      return (running) => {
        exportRuntimeContract(contract);
        const handlers = factory(running);
        dataObject(handlers, ["handlers"]);
        const names = Object.keys(contract.methods);
        if (Object.keys(handlers).some((name) => !Object.hasOwn(contract.methods, name)))
          throw new Error("Runtime handlers contain an undeclared method.");
        return Object.fromEntries(
          names.map((name) => {
            if (!Object.hasOwn(handlers, name) || typeof handlers[name] !== "function")
              throw new Error(
                `Runtime handler is missing for declared method ${JSON.stringify(name)}.`,
              );
            return [name, handlers[name]];
          }),
        );
      };
    },
    businessError<K extends keyof M & string>(
      method: K,
      details: Value<M[K]["error"]>,
    ): BusinessError<Value<M[K]["error"]>> {
      if (!Object.hasOwn(contract.methods, method)) throw new Error("Unknown Runtime method.");
      checkSchema(contract.methods[method]!.error, ["methods", method, "error"]);
      validateValue(contract.methods[method]!.error, details);
      return new BusinessError(details);
    },
  };
}
