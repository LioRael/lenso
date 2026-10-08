import { ORPCError, COMMON_ERROR_STATUS_MAP } from "@orpc/client";

export interface ProblemDefinition {
  status: number;
  title: string;
  detail: string;
}

export interface ProblemDetails extends ProblemDefinition {
  type: string;
  instance: string;
  code: string;
}

export interface ProblemDetailsOptions {
  /** Trusted, fixed public descriptions, never values copied from an error. */
  codes?: Record<string, ProblemDefinition>;
  /** May classify trusted domain errors; only configured codes are accepted. */
  mapError?: (error: unknown) => string | undefined;
  /** Safe occurrence metadata for application logging. Failures are ignored. */
  onProblem?: (problem: Readonly<ProblemDetails>) => void | Promise<void>;
}

export function validateErrorStatusMap(map: Record<string, number> | undefined): void {
  for (const status of Object.values(map ?? {})) {
    if (!Number.isSafeInteger(status) || status < 400 || status > 599) {
      throw new TypeError("Web errorStatusMap values must be safe integers from 400 to 599");
    }
  }
}

export const INTERNAL_PROBLEM: Readonly<ProblemDefinition> = Object.freeze({
  status: 500,
  title: "Internal Server Error",
  detail: "The request could not be completed.",
});

export function problemType(code: string): string {
  return `urn:lenso:problem:${code.toLowerCase().replaceAll("_", "-")}`;
}

export function createProblemDetails(options: ProblemDetailsOptions = {}) {
  const definitions: Record<string, Readonly<ProblemDefinition>> = Object.create(null);
  for (const [code, status] of Object.entries(COMMON_ERROR_STATUS_MAP)) {
    definitions[code] = Object.freeze({
      status,
      title: code
        .toLowerCase()
        .replaceAll("_", " ")
        .replace(/\b\w/g, (c) => c.toUpperCase()),
      detail: status >= 500 ? INTERNAL_PROBLEM.detail : "The request was refused.",
    });
  }
  for (const [code, definition] of Object.entries(options.codes ?? {})) {
    if (!/^[A-Z][A-Z0-9_]{0,63}$/.test(code)) {
      throw new TypeError("Problem codes must be uppercase identifiers of at most 64 characters");
    }
    validateErrorStatusMap({ [code]: definition.status });
    for (const text of [definition.title, definition.detail]) {
      if (typeof text !== "string" || text.length === 0 || text.length > 512) {
        throw new TypeError(
          "Problem titles and details must be fixed strings of 1 to 512 characters",
        );
      }
    }
    definitions[code] = Object.freeze({
      status: definition.status,
      title: definition.title,
      detail: definition.detail,
    });
  }
  // The opaque fallback cannot be reconfigured into a disclosure or a non-500 response.
  definitions.INTERNAL_SERVER_ERROR = INTERNAL_PROBLEM;
  Object.freeze(definitions);
  const errorStatusMap = Object.fromEntries(
    Object.entries(definitions).map(([code, definition]) => [code, definition.status]),
  );
  function codeFor(error: unknown): string {
    let code: string | undefined;
    try {
      code = options.mapError?.(error);
    } catch {
      return "INTERNAL_SERVER_ERROR";
    }
    code ??= error instanceof ORPCError ? error.code : undefined;
    return typeof code === "string" && Object.hasOwn(definitions, code)
      ? code
      : "INTERNAL_SERVER_ERROR";
  }
  function fromCode(code: string): ProblemDetails {
    const known =
      typeof code === "string" && Object.hasOwn(definitions, code) ? code : "INTERNAL_SERVER_ERROR";
    const definition = definitions[known]!;
    const problem = Object.freeze({
      type: problemType(known),
      ...definition,
      code: known,
      instance: `urn:uuid:${crypto.randomUUID()}`,
    });
    try {
      void Promise.resolve(options.onProblem?.(problem)).catch(() => {});
    } catch {}
    return problem;
  }
  return {
    definitions,
    errorStatusMap,
    codeFor,
    fromCode,
    fromError: (error: unknown) => fromCode(codeFor(error)),
    response(error: unknown): Response {
      const problem = fromCode(codeFor(error));
      return Response.json(problem, {
        status: problem.status,
        headers: { "content-type": "application/problem+json", "standard-server": "json" },
      });
    },
  };
}

export const problemDetailsSchema = {
  type: "object" as const,
  required: ["type", "title", "status", "detail", "instance", "code"],
  properties: {
    type: { type: "string" as const, format: "uri" },
    title: { type: "string" as const, maxLength: 512 },
    status: { type: "integer" as const, minimum: 400, maximum: 599 },
    detail: { type: "string" as const, maxLength: 512 },
    instance: { type: "string" as const, format: "uri" },
    code: { type: "string" as const, maxLength: 64 },
  },
};
