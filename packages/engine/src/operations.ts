import type { StandardJSONSchemaV1, StandardSchemaV1 } from "@standard-schema/spec";
import type { Plugin } from "lenso";
import { EngineError, environmentSecrets, redact, type SourceLocation } from "./diagnostics";

export interface Operation {
  readonly plugin: Plugin<unknown>;
  readonly method: string;
  readonly description: string;
  readonly input: StandardSchemaV1;
  readonly source?: SourceLocation;
  readonly effect?: "read" | "write" | "unknown";
}
/** Static adapter metadata only: invocation always uses the existing service method. */
export function defineOperation<T, S extends StandardSchemaV1>(
  operation: Omit<Operation, "plugin" | "method" | "input"> & {
    plugin: Plugin<T>;
    method: {
      [K in keyof T]: T[K] extends (input: StandardSchemaV1.InferOutput<S>) => unknown ? K : never;
    }[keyof T] &
      string;
    input: S;
  },
): Operation {
  return operation;
}

export function validateOperations(
  plugins: readonly Plugin<unknown>[],
  value: unknown,
): readonly Operation[] {
  if (!Array.isArray(value))
    throw new EngineError({
      code: "invalid-operations",
      phase: "discovery",
      message: "Config operations must be an array of explicit service declarations.",
    });
  const keys = new Map<Plugin<unknown>, Set<string>>();
  for (const item of value) {
    if (
      !item ||
      !plugins.includes(item.plugin) ||
      typeof item.method !== "string" ||
      !item.method ||
      ["__proto__", "constructor", "prototype"].includes(item.method) ||
      typeof item.description !== "string" ||
      item.input?.["~standard"]?.version !== 1 ||
      typeof item.input["~standard"].validate !== "function"
    ) {
      throw new EngineError({
        code: "invalid-operations",
        phase: "discovery",
        message:
          "Each operation needs the exact installed plugin, a service method, description and Standard Schema input.",
      });
    }
    const methods = keys.get(item.plugin) ?? new Set<string>();
    if (methods.has(item.method))
      throw new EngineError({
        code: "duplicate-operation",
        phase: "discovery",
        message: "Operation declarations must be unique per plugin and method.",
      });
    methods.add(item.method);
    keys.set(item.plugin, methods);
  }
  return value;
}

export function describeOperation(operation: Operation, configPath: string) {
  const standard = operation.input["~standard"] as StandardSchemaV1.Props &
    Partial<StandardJSONSchemaV1.Props>;
  let inputSchema: Record<string, unknown> | null = null;
  try {
    inputSchema = standard.jsonSchema?.input({ target: "draft-2020-12" }) ?? null;
  } catch {
    /* Some schemas cannot be represented statically. */
  }
  return {
    pluginId: operation.plugin.id,
    method: operation.method,
    description: operation.description,
    effect: operation.effect ?? "unknown",
    source: operation.source ?? { file: configPath },
    inputSchema: inputSchema ? safeInputSchema(inputSchema) : null,
    schemaAvailability: inputSchema ? "available" : "runtime-validation-only",
  };
}

/** Preserve field names/types; omit payload annotations that may embed credentials. */
function safeInputSchema(value: Record<string, unknown>): Record<string, unknown> {
  const secrets = environmentSecrets();
  function walk(item: unknown, fields = false): unknown {
    if (Array.isArray(item)) return item.map((child) => walk(child));
    if (item && typeof item === "object")
      return Object.fromEntries(
        Object.entries(item)
          .filter(
            ([key]) => fields || (key !== "default" && key !== "examples" && key !== "example"),
          )
          .map(([key, child]) => [
            key,
            walk(
              child,
              [
                "properties",
                "$defs",
                "definitions",
                "patternProperties",
                "dependentSchemas",
              ].includes(key),
            ),
          ]),
      );
    return typeof item === "string" ? redact(item, secrets) : item;
  }
  return walk(value) as Record<string, unknown>;
}

export function redactOperationDescription(description: ReturnType<typeof describeOperation>) {
  const { inputSchema, ...metadata } = description;
  return { ...(redact(metadata, environmentSecrets()) as typeof metadata), inputSchema };
}
