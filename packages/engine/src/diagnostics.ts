import { ConfigError, DiagnosticError, lifecycleFailure } from "@lenso/core";

export interface SourceLocation {
  readonly file: string;
  readonly export?: string;
  readonly line?: number;
  readonly column?: number;
}
export interface EngineDiagnostic {
  readonly code: string;
  readonly phase: string;
  readonly message: string;
  readonly instanceId?: string;
  readonly pluginId?: string;
  readonly dependencyId?: string;
  readonly operation?: string;
  readonly source?: SourceLocation;
  readonly details?: unknown;
  readonly causes?: readonly EngineDiagnostic[];
}
export class EngineError extends Error {
  readonly diagnostic: EngineDiagnostic;

  constructor(detail: EngineDiagnostic, options?: ErrorOptions) {
    super(detail.message, options);
    this.diagnostic = detail;
    this.name = "EngineError";
  }
}

/** Unknown application errors are opaque: they may contain inputs or credentials. */
export function diagnostic(
  error: unknown,
  fallback: Partial<EngineDiagnostic> = {},
): EngineDiagnostic {
  return describeError(
    error,
    fallback,
    Object.assign(new WeakSet<object>(), { remaining: 256 }),
    0,
  );
}

const diagnosticLimit = 32;
const configDetailCodes = new Set([
  "config-source-failed",
  "config-invalid-data",
  "config-invalid",
  "config-cancelled",
  "config-env-invalid",
  "config-file-missing",
  "config-file-invalid",
]);
type DiagnosticTraversal = WeakSet<object> & { remaining: number };
function safePaths(value: unknown): (string | number)[][] {
  if (!Array.isArray(value)) return [];
  return value
    .slice(0, diagnosticLimit)
    .filter(
      (path) =>
        Array.isArray(path) &&
        path.length <= 8 &&
        path.every(
          (segment) =>
            (typeof segment === "string" && segment.length <= 128) ||
            (typeof segment === "number" && Number.isSafeInteger(segment) && segment >= 0),
        ),
    );
}
function safeDetail(
  detail: EngineDiagnostic,
  seen: DiagnosticTraversal,
  depth: number,
): EngineDiagnostic {
  if (--seen.remaining < 0)
    return { code: "runtime-failed", phase: "runtime", message: "Additional diagnostics omitted." };
  const result: Record<string, unknown> = {};
  for (const key of [
    "code",
    "phase",
    "message",
    "instanceId",
    "pluginId",
    "dependencyId",
    "operation",
  ] as const)
    if (typeof detail[key] === "string")
      result[key] = detail[key].slice(0, key === "message" ? 1024 : 256);
  if (detail.source && typeof detail.source.file === "string")
    result.source = {
      file: detail.source.file.slice(0, 1024),
      ...(typeof detail.source.export === "string"
        ? { export: detail.source.export.slice(0, 256) }
        : {}),
      ...(Number.isSafeInteger(detail.source.line) ? { line: detail.source.line } : {}),
      ...(Number.isSafeInteger(detail.source.column) ? { column: detail.source.column } : {}),
    };
  const details = detail.details;
  if (details && typeof details === "object") {
    if (detail.code === "invalid-input")
      result.details = { paths: safePaths(Reflect.get(details, "paths")) };
    else if (detail.code === "invalid-plugin")
      result.details = { path: safePaths([Reflect.get(details, "path")])[0] ?? [] };
    else if (
      ["ambiguous-application-target", "missing-application-selection"].includes(detail.code)
    ) {
      const candidates = Reflect.get(details, "candidates");
      if (Array.isArray(candidates))
        result.details = {
          candidates: candidates
            .filter((path) => typeof path === "string")
            .slice(0, diagnosticLimit)
            .map((path) => path.slice(0, 1024)),
        };
    } else if (detail.phase === "config" && configDetailCodes.has(detail.code))
      result.details = {
        ...(safePaths([Reflect.get(details, "path")])[0]
          ? { path: safePaths([Reflect.get(details, "path")])[0] }
          : {}),
        ...(typeof Reflect.get(details, "sourceId") === "string"
          ? { sourceId: Reflect.get(details, "sourceId").slice(0, 256) }
          : {}),
      };
    else if (detail.code === "duplicate-id") {
      const sources = Reflect.get(details, "declaringSources");
      if (Array.isArray(sources))
        result.details = {
          declaringSources: sources
            .slice(0, diagnosticLimit)
            .flatMap((source) =>
              source && typeof source.file === "string"
                ? [safeDetail({ code: "", phase: "", message: "", source }, seen, depth).source]
                : [],
            ),
        };
    }
  }
  if (Array.isArray(detail.causes) && depth < 8)
    result.causes = detail.causes.slice(0, diagnosticLimit).flatMap((cause) => {
      if (!cause || typeof cause !== "object" || seen.has(cause) || seen.remaining <= 0) return [];
      seen.add(cause);
      return [safeDetail(cause, seen, depth + 1)];
    });
  return result as unknown as EngineDiagnostic;
}
function sameProjection(left: EngineDiagnostic, right: EngineDiagnostic): boolean {
  const project = (detail: EngineDiagnostic) => {
    const { causes: _, ...fields } = safeDetail(
      detail,
      Object.assign(new WeakSet<object>(), { remaining: 256 }),
      0,
    );
    return stableJson(fields);
  };
  return project(left) === project(right);
}
function describeError(
  error: unknown,
  fallback: Partial<EngineDiagnostic>,
  seen: DiagnosticTraversal,
  depth: number,
): EngineDiagnostic {
  const lifetime = lifecycleFailure(error);
  const phase = lifetime?.phase ?? fallback.phase ?? "runtime";
  const base = safeDetail(
    {
      code:
        phase === "setup"
          ? "initialization-failed"
          : phase === "cleanup"
            ? "cleanup-failed"
            : "runtime-failed",
      phase,
      message: `Operation failed during ${phase}. Application error text is omitted.`,
      ...fallback,
      ...lifetime,
    },
    seen,
    depth,
  );
  if (error && typeof error === "object") {
    if (seen.has(error) || depth >= 8 || seen.remaining <= 0) return base;
    seen.add(error);
  }
  const context = {
    ...(base.source ? { source: base.source } : {}),
    ...(base.operation ? { operation: base.operation } : {}),
    phase,
  };
  if (error instanceof EngineError) {
    return {
      ...base,
      ...safeDetail(error.diagnostic, seen, depth),
      ...(error.diagnostic.causes ||
      error.cause === undefined ||
      (error.cause instanceof EngineError &&
        sameProjection(error.diagnostic, error.cause.diagnostic))
        ? {}
        : error.cause instanceof EngineError ||
            error.cause instanceof AggregateError ||
            error.diagnostic.code.startsWith("engine-")
          ? { causes: [describeError(error.cause, context, seen, depth + 1)] }
          : {}),
    };
  }
  if (error instanceof DiagnosticError) {
    return {
      ...base,
      code: "invalid-assembly",
      phase: "assembly",
      message: "Plugin assembly is invalid.",
      causes: error.diagnostics.slice(0, diagnosticLimit).map((item) =>
        safeDetail(
          {
            ...item,
            phase: "assembly",
            ...(item.source ? { source: item.source } : base.source ? { source: base.source } : {}),
          },
          seen,
          depth + 1,
        ),
      ),
    };
  }
  if (error instanceof ConfigError) {
    return {
      ...base,
      code: "config-invalid",
      phase: "config",
      message: "Application configuration failed before plugin setup.",
      causes: error.diagnostics.slice(0, diagnosticLimit).map((item) =>
        safeDetail(
          {
            code: item.code,
            phase: "config",
            message: "Configuration could not be resolved or validated.",
            pluginId: item.pluginId,
            ...(item.source ? { source: item.source } : {}),
            details: {
              ...(item.path ? { path: item.path } : {}),
              ...(item.sourceId ? { sourceId: item.sourceId } : {}),
            },
          },
          seen,
          depth + 1,
        ),
      ),
    };
  }
  if (error instanceof AggregateError) {
    return {
      ...base,
      causes: error.errors
        .slice(0, diagnosticLimit)
        .map((cause) => describeError(cause, context, seen, depth + 1)),
    };
  }
  if (error instanceof Error && error.cause !== undefined)
    return { ...base, causes: [describeError(error.cause, context, seen, depth + 1)] };
  return base;
}

const sensitiveKey =
  /password|passwd|secret|token|authorization|cookie|credential|api[-_]?key|connection[-_]?string/i;
/** Best-effort redaction for trusted app logs/results, never a sandbox. */
export function redact(
  value: unknown,
  secrets: readonly string[] = [],
  seen = new WeakSet<object>(),
): unknown {
  if (typeof value === "string") {
    let result = value
      .replace(/(\b(?:Bearer|Basic)\s+)\S+/gi, "$1[REDACTED]")
      .replace(/(:\/\/)[^\s/@]+@/g, "$1[REDACTED]@")
      .replace(
        /((?:password|passwd|secret|token|credential|api[-_]?key|connection[-_]?string)[/\\])[^\s?#]+/gi,
        "$1[REDACTED]",
      )
      .replace(/((?:password|secret|token|api[-_]?key)\s*[=:]\s*)[^\s,;]+/gi, "$1[REDACTED]");
    for (const secret of secrets)
      if (secret.length >= 3) result = result.replaceAll(secret, "[REDACTED]");
    return result;
  }
  if (!value || typeof value !== "object") return value;
  if (seen.has(value)) return "[Circular]";
  seen.add(value);
  const result = Array.isArray(value)
    ? value.map((item) => redact(item, secrets, seen))
    : Object.fromEntries(
        Object.entries(value).map(([key, item]) => [
          key,
          sensitiveKey.test(key) ? "[REDACTED]" : redact(item, secrets, seen),
        ]),
      );
  seen.delete(value);
  return result;
}

/** Reject lossy JSON rather than returning a successful undefined/null replacement. */
export function stableJson(value: unknown, space?: number): string {
  const seen = new WeakSet<object>();
  function normalize(item: unknown): unknown {
    if (item === null || typeof item === "string" || typeof item === "boolean") return item;
    if (typeof item === "number" && Number.isFinite(item)) return item;
    if (typeof item !== "object" || seen.has(item))
      throw new EngineError({
        code: "serialization-failed",
        phase: "output",
        message: "Output must be finite, acyclic JSON data.",
      });
    if (
      Symbol.asyncIterator in item ||
      Reflect.ownKeys(item).some((key) => typeof key === "symbol")
    )
      throw new EngineError({
        code: "serialization-failed",
        phase: "output",
        message: "Output cannot contain streams or symbol-keyed data.",
      });
    if (
      !Array.isArray(item) &&
      Object.getPrototypeOf(item) !== Object.prototype &&
      Object.getPrototypeOf(item) !== null
    )
      throw new EngineError({
        code: "serialization-failed",
        phase: "output",
        message: "Output must contain plain JSON objects.",
      });
    seen.add(item);
    const normalized = Array.isArray(item)
      ? Array.from(item, normalize)
      : Object.fromEntries(
          Object.keys(item)
            .sort()
            .map((key) => [key, normalize(Reflect.get(item, key))]),
        );
    seen.delete(item);
    return normalized;
  }
  return JSON.stringify(normalize(value), null, space);
}

export function environmentSecrets(): string[] {
  if (typeof process === "undefined") return [];
  return Object.entries(process.env)
    .filter(([key, value]) => sensitiveKey.test(key) && value)
    .map(([, value]) => value!);
}
