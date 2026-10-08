import type { StandardJSONSchemaV1, StandardSchemaV1 } from "@standard-schema/spec";
import type { Plugin, RunningApp } from "@lenso/core";
import { metrics, SpanStatusCode, trace } from "@opentelemetry/api";
import { EngineError, environmentSecrets, redact, type SourceLocation } from "./diagnostics";

export interface Operation {
  readonly plugin: Plugin<unknown>;
  readonly method: string;
  readonly description: string;
  readonly input: StandardSchemaV1;
  readonly source?: SourceLocation;
  readonly effect?: "read" | "write" | "unknown";
  readonly destructive?: boolean;
  readonly outputDescription?: string;
  readonly retry?: "safe" | "unsafe" | "unknown";
  readonly cancellation?: "cooperative" | "request-only" | "none" | "unknown";
}

/** Shared by entries/adapters after their input and authorization boundary. */
export async function executeOperation(
  running: RunningApp,
  operation: Operation,
  input: unknown,
): Promise<unknown> {
  const attributes = {
    "lenso.instance.id": running.instanceId,
    "lenso.plugin.id": operation.plugin.id,
    "lenso.operation": operation.method,
  };
  const meter = metrics.getMeter("@lenso/engine");
  const labels = { outcome: "success" };
  const started = performance.now();
  return trace
    .getTracer("@lenso/engine")
    .startActiveSpan("lenso.operation", { attributes }, async (span) => {
      try {
        const service = running.get(operation.plugin);
        if (
          service === null ||
          typeof service !== "object" ||
          !Object.hasOwn(service, operation.method) ||
          typeof Reflect.get(service, operation.method) !== "function"
        )
          throw new EngineError({
            code: "unavailable-operation",
            phase: "invoke",
            message: "Declared operation is not an own callable service method.",
            pluginId: operation.plugin.id,
          });
        return await Reflect.get(service, operation.method).call(service, input);
      } catch (error) {
        labels.outcome = "failure";
        span.setStatus({ code: SpanStatusCode.ERROR });
        meter.createCounter("lenso.operation.errors").add(1);
        throw error;
      } finally {
        try {
          running.logger?.debug(
            {
              instanceId: running.instanceId,
              pluginId: operation.plugin.id,
              operation: operation.method,
              outcome: labels.outcome,
            },
            "Operation completed",
          );
        } catch {
          // Diagnostics cannot change the service result or failure identity.
        }
        meter.createCounter("lenso.operation.calls").add(1, labels);
        meter
          .createHistogram("lenso.operation.duration", { unit: "ms" })
          .record(performance.now() - started, labels);
        span.end();
      }
    });
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
      (item.effect !== undefined && !["read", "write", "unknown"].includes(item.effect)) ||
      (item.destructive !== undefined && typeof item.destructive !== "boolean") ||
      (item.outputDescription !== undefined && typeof item.outputDescription !== "string") ||
      (item.retry !== undefined && !["safe", "unsafe", "unknown"].includes(item.retry)) ||
      (item.cancellation !== undefined &&
        !["cooperative", "request-only", "none", "unknown"].includes(item.cancellation)) ||
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
    destructive: operation.destructive ?? null,
    outputDescription: operation.outputDescription ?? null,
    retry: operation.retry ?? "unknown",
    cancellation: operation.cancellation ?? "unknown",
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
