import { DiagnosticError, lifecycleFailure } from "lenso";

export interface SourceLocation {
  readonly file: string;
  readonly export?: string;
  readonly line?: number;
  readonly column?: number;
}
export interface CliDiagnostic {
  readonly code: string;
  readonly phase: string;
  readonly message: string;
  readonly pluginId?: string;
  readonly operation?: string;
  readonly source?: SourceLocation;
  readonly details?: unknown;
  readonly causes?: readonly CliDiagnostic[];
}
export class CliError extends Error {
  constructor(
    readonly diagnostic: CliDiagnostic,
    readonly exitCode = 1,
    options?: ErrorOptions,
  ) {
    super(diagnostic.message, options);
    this.name = "CliError";
  }
}

/** Unknown application errors are opaque: they may contain inputs or credentials. */
export function diagnostic(error: unknown, fallback: Partial<CliDiagnostic> = {}): CliDiagnostic {
  const lifetime = lifecycleFailure(error);
  const phase = lifetime?.phase ?? fallback.phase ?? "runtime";
  const base = {
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
  };
  if (error instanceof CliError) {
    return {
      ...base,
      ...error.diagnostic,
      ...(error.cause === undefined ? {} : { causes: [diagnostic(error.cause, base)] }),
    };
  }
  if (error instanceof DiagnosticError) {
    return {
      ...base,
      code: "invalid-assembly",
      phase: "assembly",
      message: "Plugin assembly is invalid.",
      causes: error.diagnostics.map((item) => ({
        ...item,
        phase: "assembly",
        source: base.source,
      })),
    };
  }
  if (error instanceof AggregateError) {
    return {
      ...base,
      causes: error.errors.map((cause) =>
        diagnostic(cause, { source: base.source, operation: base.operation, phase }),
      ),
    };
  }
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
      .replace(/([a-z][a-z0-9+.-]*:\/\/)[^\s/@]+:[^\s/@]+@/gi, "$1[REDACTED]@")
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
      throw new CliError({
        code: "serialization-failed",
        phase: "output",
        message: "Output must be finite, acyclic JSON data.",
      });
    if (
      !Array.isArray(item) &&
      Object.getPrototypeOf(item) !== Object.prototype &&
      Object.getPrototypeOf(item) !== null
    )
      throw new CliError({
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
  return Object.entries(process.env)
    .filter(([key, value]) => sensitiveKey.test(key) && value)
    .map(([, value]) => value!);
}
