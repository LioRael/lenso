import { EngineError, diagnostic } from "@lenso/engine/diagnostics";

export {
  diagnostic,
  environmentSecrets,
  redact,
  stableJson,
  type EngineDiagnostic as CliDiagnostic,
  type SourceLocation,
} from "@lenso/engine/diagnostics";

/** CLI policy stays outside the programmatic Engine error contract. */
export function exitCode(error: unknown): number {
  if (error instanceof CliError) return error.exitCode;
  const { phase } = diagnostic(error);
  if (phase === "arguments" || phase === "input") return 2;
  if (["discovery", "assembly", "engine-config"].includes(phase)) return 3;
  return 1;
}

export class CliError extends EngineError {
  readonly exitCode: number;

  constructor(
    detail: ConstructorParameters<typeof EngineError>[0],
    status = 1,
    options?: ErrorOptions,
  ) {
    super(detail, options);
    this.exitCode = status;
    this.name = "CliError";
  }
}
