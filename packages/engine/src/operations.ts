import type { StandardJSONSchemaV1, StandardSchemaV1 } from "@standard-schema/spec";
import type { Plugin, RunningApp } from "@lenso/core";
import { metrics, SpanStatusCode, trace } from "@opentelemetry/api";
import {
  EngineError,
  environmentSecrets,
  redact,
  stableJson,
  type EngineDiagnostic,
  type SourceLocation,
} from "./diagnostics";

declare const operationContext: unique symbol;

export interface Operation<C = unknown> {
  readonly plugin: Plugin<unknown>;
  readonly method: string;
  readonly description: string;
  readonly input: StandardSchemaV1;
  readonly mapError?: (error: unknown) => EngineDiagnostic | undefined;
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
  running: RunningApp,
  operation: Operation,
  input: unknown,
  context?: unknown,
): Promise<unknown> {
  const attributes = {
    "lenso.instance.id": running.instanceId,
    "lenso.plugin.id": operation.plugin.id,
    "lenso.operation": operation.method,
  };
  const labels = { outcome: "success" };
  const started = performance.now();
  const collect = (action: () => void) => {
    try {
      action();
    } catch {
      /* Optional telemetry cannot change business outcomes. */
    }
  };
  const execute = async (span?: import("@opentelemetry/api").Span) => {
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
      try {
        return operation.context
          ? await method.call(service, input, context)
          : await method.call(service, input);
      } catch (error) {
        throw operationError(operation, error);
      }
    } catch (error) {
      labels.outcome = "failure";
      collect(() => span?.setStatus({ code: SpanStatusCode.ERROR }));
      collect(() =>
        metrics.getMeter("@lenso/engine").createCounter("lenso.operation.errors").add(1),
      );
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
      collect(() =>
        metrics.getMeter("@lenso/engine").createCounter("lenso.operation.calls").add(1, labels),
      );
      collect(() =>
        metrics
          .getMeter("@lenso/engine")
          .createHistogram("lenso.operation.duration", { unit: "ms" })
          .record(performance.now() - started, labels),
      );
      collect(() => span?.end());
    }
  };
  let execution: Promise<unknown> | undefined;
  try {
    return trace
      .getTracer("@lenso/engine")
      .startActiveSpan("lenso.operation", { attributes }, (span) => (execution = execute(span)));
  } catch {
    return execution ?? execute();
  }
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

/** Only trusted application projectors classify actual domain failures. */
export function operationError(operation: Operation, error: unknown): unknown {
  if (error instanceof EngineError) return error;
  try {
    const mapped = operation.mapError?.(error);
    if (
      mapped &&
      typeof mapped.code === "string" &&
      typeof mapped.phase === "string" &&
      typeof mapped.message === "string"
    )
      return new EngineError({ ...operationLocation(operation), ...mapped }, { cause: error });
  } catch {
    // A broken projector cannot replace the original failure.
  }
  return error;
}

function inputIssuePaths(
  operation: Operation,
  issues: readonly StandardSchemaV1.Issue[],
): string[][] {
  try {
    const standard = operation.input["~standard"] as StandardSchemaV1.Props &
      Partial<StandardJSONSchemaV1.Props>;
    const schema = standard.jsonSchema?.input({ target: "draft-2020-12" });
    if (!schema) return [];
    const declared = new Set<string>();
    let visits = 0;
    function visit(node: unknown, path: string[], depth: number) {
      if (!node || typeof node !== "object" || depth > 8 || ++visits > 256) return;
      const properties = Reflect.get(node, "properties");
      if (properties && typeof properties === "object")
        for (const key of Object.keys(properties).slice(0, 256)) {
          if (declared.size >= 256) break;
          if (key.length > 128 || path.length >= 8) continue;
          const child = [...path, key];
          declared.add(JSON.stringify(child));
          visit(Reflect.get(properties, key), child, depth + 1);
        }
      for (const keyword of ["anyOf", "oneOf", "allOf"]) {
        const alternatives = Reflect.get(node, keyword);
        if (Array.isArray(alternatives))
          for (const child of alternatives.slice(0, 32)) visit(child, path, depth + 1);
      }
    }
    visit(schema, [], 0);
    return issues.slice(0, 32).flatMap((issue) => {
      if ((issue.path?.length ?? 0) > 8) return [];
      const path = (issue.path ?? []).map((segment) =>
        typeof segment === "object" ? segment.key : segment,
      );
      if (path.length > 8 || !path.every((key) => typeof key === "string" && key.length <= 128))
        return [];
      return declared.has(JSON.stringify(path)) ? [path as string[]] : [];
    });
  } catch {
    return [];
  }
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
        paths: inputIssuePaths(operation, validated.issues),
      },
    });
  return validated.value;
}

/** Entries own admission and lifecycle; this function never queues, retries or stops an app. */
export async function invokeValidatedOperation<O extends Operation>(
  running: RunningApp,
  operation: O,
  validatedInput: unknown,
  options: OperationInvocationOptions<NoInfer<OperationContext<O>>> = {},
): Promise<unknown> {
  const location = { ...operationLocation(operation), instanceId: running.instanceId };
  try {
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
    if (
      operation.confirmation === "required" &&
      (!options.confirm || (await options.confirm()) !== true)
    )
      throw new EngineError({
        code: "confirmation-required",
        phase: "invoke",
        message: "Entry cannot verify the required user confirmation.",
        ...location,
      });
    if (
      operation.approval === "required" &&
      (!options.approve || (await options.approve()) !== true)
    )
      throw new EngineError({
        code: "approval-required",
        phase: "invoke",
        message: "Entry cannot verify approval from the configured approval owner.",
        ...location,
      });
    const result = await executeOperation(running, operation, validatedInput, options.context);
    // Check the original result before redaction, which otherwise hides cycles and non-JSON values.
    const raw = boundedJson(result, maxBytes);
    const safe = redact(JSON.parse(raw), environmentSecrets());
    boundedJson(safe, maxBytes);
    return safe;
  } catch (cause) {
    if (cause instanceof EngineError) throw cause;
    throw new EngineError(
      {
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
      (item.mapError !== undefined && typeof item.mapError !== "function") ||
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
