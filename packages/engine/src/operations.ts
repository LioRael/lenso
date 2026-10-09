import type { StandardJSONSchemaV1, StandardSchemaV1 } from "@standard-schema/spec";
import type { Plugin, RunningApp } from "@lenso/core";
import { metrics, SpanStatusCode, trace } from "@opentelemetry/api";
import {
  EngineError,
  diagnostic,
  environmentSecrets,
  redact,
  stableJson,
  type SourceLocation,
} from "./diagnostics";

declare const operationContext: unique symbol;

export type OperationRuntime = Pick<RunningApp, "instanceId" | "get" | "logger">;

export interface Operation<C = unknown> {
  readonly plugin: Plugin<unknown>;
  readonly method: string;
  readonly description: string;
  readonly input: StandardSchemaV1;
  readonly context?: true;
  readonly [operationContext]?: C;
  readonly confirmation?: "required";
  readonly approval?: "required";
  readonly source?: SourceLocation;
  readonly effect?: "read" | "write" | "unknown";
  readonly destructive?: boolean;
  readonly outputDescription?: string;
  readonly retry?: "safe" | "unsafe" | "unknown";
  readonly cancellation?: "cooperative" | "request-only" | "none" | "unknown";
}

/** Shared by entries/adapters after their input and authorization boundary. */
export async function executeOperation(
  running: OperationRuntime,
  operation: Operation,
  input: unknown,
  context?: unknown,
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
            operation: `${operation.plugin.id}.${operation.method}`,
          });
        const method = Reflect.get(service, operation.method);
        return operation.context
          ? await method.call(service, input, context)
          : await method.call(service, input);
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
export function defineOperation<T, S extends StandardSchemaV1, const K extends keyof T & string>(
  operation: Omit<Operation, "plugin" | "method" | "input" | "context"> & {
    plugin: Plugin<T>;
    method: K &
      (T[K] extends (input: StandardSchemaV1.InferOutput<S>) => unknown ? unknown : never);
    input: S;
    context?: never;
  },
): Operation<void> & { readonly plugin: Plugin<T>; readonly method: K; readonly input: S };
export function defineOperation<T, S extends StandardSchemaV1, const K extends keyof T & string>(
  operation: Omit<Operation, "plugin" | "method" | "input" | "context"> & {
    plugin: Plugin<T>;
    method: K &
      (T[K] extends (input: StandardSchemaV1.InferOutput<S>, context: never) => unknown
        ? unknown
        : never) &
      (T[K] extends (...args: infer A) => unknown
        ? Exclude<A[1], undefined> extends never
          ? never
          : unknown
        : never);
    input: S;
    context: true;
  },
): Operation<T[K] extends (...args: infer A) => unknown ? Exclude<A[1], undefined> : never> & {
  readonly plugin: Plugin<T>;
  readonly method: K;
  readonly input: S;
  readonly context: true;
};
export function defineOperation(operation: Operation): Operation {
  return operation;
}

export type OperationContext<O extends Operation> = O extends Operation<infer C> ? C : never;

export interface OperationInvocationOptions<C = unknown> {
  readonly context?: C;
  readonly signal?: AbortSignal;
  readonly maxOutputBytes?: number;
  /** Trusted entry callbacks verify this invocation, not booleans from business input. */
  readonly confirm?: () => boolean | Promise<boolean>;
  readonly approve?: () => boolean | Promise<boolean>;
}

export type OperationBoundOptions<O extends Operation> = [O] extends [never]
  ? OperationInvocationOptions
  : OperationInvocationOptions<OperationContext<O>> &
      (O extends { readonly context: true } ? { readonly context: OperationContext<O> } : unknown);

export type OperationBinding<O extends Operation = Operation> = (
  operation: O,
  validatedInput: unknown,
  running: RunningApp,
) => OperationBoundOptions<O> | Promise<OperationBoundOptions<O>>;

function operationLocation(operation: Operation) {
  return {
    pluginId: operation.plugin.id,
    operation: `${operation.plugin.id}.${operation.method}`,
    ...(operation.source ? { source: operation.source } : {}),
  };
}

/** Validate finite JSON and its byte budget without altering schema field names. */
export function boundedJson(value: unknown, maxBytes = 1024 * 1024): string {
  if (!Number.isSafeInteger(maxBytes) || maxBytes < 1)
    throw new EngineError({
      code: "invalid-arguments",
      phase: "arguments",
      message: "Output limit must be a positive safe integer.",
    });
  const json = stableJson(value);
  if (new TextEncoder().encode(json).byteLength > maxBytes)
    throw new EngineError({
      code: "output-too-large",
      phase: "output",
      message: "Operation output exceeds the entry limit.",
    });
  return json;
}

export function resolveOperation(
  plugins: readonly Plugin<unknown>[],
  operations: readonly Operation[],
  pluginId: string,
  method: string,
): Operation {
  const plugin = plugins.find((candidate) => candidate.id === pluginId);
  if (!plugin)
    throw new EngineError({
      code: "unknown-plugin",
      phase: "discovery",
      message: "Unknown plugin.",
      pluginId,
    });
  const operation = validateOperations(plugins, operations).find(
    (candidate) => candidate.plugin === plugin && candidate.method === method,
  );
  if (!operation)
    throw new EngineError({
      code: "unknown-operation",
      phase: "discovery",
      message: "Service method is not explicitly exposed for this entry.",
      pluginId,
      operation: `${pluginId}.${method}`,
    });
  return operation;
}

/** Validate raw input once, including schemas that transform their output. */
export async function validateOperationInput(
  operation: Operation,
  input: unknown,
): Promise<unknown> {
  const location = operationLocation(operation);
  let validated;
  try {
    validated = await operation.input["~standard"].validate(input);
  } catch (cause) {
    throw new EngineError(
      {
        code: "invalid-input",
        phase: "input",
        message: "Input validation failed.",
        ...location,
      },
      { cause },
    );
  }
  if (validated.issues)
    throw new EngineError({
      code: "invalid-input",
      phase: "input",
      message: "Input does not satisfy the shared service schema.",
      ...location,
      details: {
        paths: validated.issues.map((issue) =>
          (issue.path ?? []).map((segment) =>
            String(typeof segment === "object" ? segment.key : segment),
          ),
        ),
      },
    });
  return validated.value;
}

/** Entries own admission and lifecycle; this function never queues, retries or stops an app. */
export async function invokeValidatedOperation<O extends Operation>(
  running: OperationRuntime,
  operation: O,
  validatedInput: unknown,
  options: OperationInvocationOptions<NoInfer<OperationContext<O>>> = {},
): Promise<unknown> {
  const location = { ...operationLocation(operation), instanceId: running.instanceId };
  try {
    options.signal?.throwIfAborted();
    const maxBytes = options.maxOutputBytes ?? 1024 * 1024;
    if (!Number.isSafeInteger(maxBytes) || maxBytes < 1)
      throw new EngineError({
        code: "invalid-arguments",
        phase: "arguments",
        message: "Output limit must be a positive safe integer.",
        ...location,
      });
    if (operation.context && options.context === undefined)
      throw new EngineError({
        code: "missing-context-binding",
        phase: "invoke",
        message: "Operation requires a trusted entry context binding.",
        ...location,
      });
    if (operation.confirmation === "required") {
      const confirmed = options.confirm && (await options.confirm());
      options.signal?.throwIfAborted();
      if (confirmed !== true)
        throw new EngineError({
          code: "confirmation-required",
          phase: "invoke",
          message: "Entry cannot verify the required user confirmation.",
          ...location,
        });
    }
    if (operation.approval === "required") {
      const approved = options.approve && (await options.approve());
      options.signal?.throwIfAborted();
      if (approved !== true)
        throw new EngineError({
          code: "approval-required",
          phase: "invoke",
          message: "Entry cannot verify approval from the configured approval owner.",
          ...location,
        });
    }
    options.signal?.throwIfAborted();
    const result = await executeOperation(running, operation, validatedInput, options.context);
    options.signal?.throwIfAborted();
    // Check the original result before redaction, which otherwise hides cycles and non-JSON values.
    const raw = boundedJson(result, maxBytes);
    const safe = redact(JSON.parse(raw), environmentSecrets());
    boundedJson(safe, maxBytes);
    return safe;
  } catch (cause) {
    throw new EngineError(
      cause instanceof EngineError
        ? diagnostic(cause, { ...location, phase: "invoke" })
        : {
            code: "invocation-failed",
            phase: "invoke",
            message: "Service invocation failed.",
            ...location,
          },
      { cause },
    );
  }
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
      (item.context !== undefined && item.context !== true) ||
      (item.confirmation !== undefined && item.confirmation !== "required") ||
      (item.approval !== undefined && item.approval !== "required") ||
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
    contextRequired: operation.context === true,
    confirmation: operation.confirmation ?? null,
    approval: operation.approval ?? null,
    source: operation.source ?? { file: configPath },
    inputSchema: inputSchema ? safeInputSchema(inputSchema) : null,
    schemaAvailability: inputSchema ? "available" : "runtime-validation-only",
  };
}

/** Preserve field names/types; omit payload annotations that may embed credentials. */
export function safeInputSchema(
  value: Record<string, unknown>,
  secrets: readonly string[] = environmentSecrets(),
): Record<string, unknown> {
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
