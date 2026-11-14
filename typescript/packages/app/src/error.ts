import { publicInputPath } from "./metadata.js";
import type { JsonSchema } from "./metadata.js";

/** Stable classifications shared by native execution and external adapters. */
export type ErrorCode =
  | "invalid_input"
  | "unauthenticated"
  | "forbidden"
  | "not_found"
  | "conflict"
  | "cancelled"
  | "internal";

export interface InputIssue {
  readonly path: readonly (string | number)[];
  readonly code: "invalid_value" | "required" | "out_of_range" | "invalid_format";
}

export interface PublicError {
  readonly code: ErrorCode;
  readonly message: string;
  readonly issues?: readonly InputIssue[];
}

export class ActionError extends Error {
  readonly issues: readonly InputIssue[] | undefined;
  constructor(
    readonly code: ErrorCode,
    message: string,
    options?: ErrorOptions & { readonly issues?: readonly InputIssue[] },
  ) {
    super(message, options);
    this.name = "ActionError";
    this.issues = code === "invalid_input" && options?.issues?.length ? structuredClone(options.issues) : undefined;
  }

  /** The only external error projection; private causes and stacks stay native. */
  toJSON(): PublicError {
    return { code: this.code, message: this.message, ...(this.issues === undefined ? {} : { issues: structuredClone(this.issues) }) };
  }
}

/** Parser/codec causes are opaque. Only structural issue paths can enter the public projection. */
export function inputError(cause: unknown, schema?: JsonSchema): ActionError {
  const issues: InputIssue[] = [];
  try {
    if (cause instanceof ActionError && cause.code === "invalid_input" && cause.issues) {
      return new ActionError("invalid_input", "Invalid input", { cause, issues: cause.issues });
    }
    if (typeof cause === "object" && cause !== null && "issues" in cause && Array.isArray(cause.issues)) {
      for (const issue of cause.issues.slice(0, 16)) {
        if (typeof issue !== "object" || issue === null || !("path" in issue) || !Array.isArray(issue.path)) continue;
        const path: unknown[] = issue.path;
        if (!path.every((segment): segment is string | number => typeof segment === "string" || typeof segment === "number")) continue;
        issues.push({ path: publicInputPath(path, schema), code: "invalid_value" });
      }
    }
  } catch {
    // Optional parser diagnostics cannot change the original input failure.
    issues.length = 0;
  }
  if (!issues.length) issues.push({ path: [], code: "invalid_value" });
  return new ActionError("invalid_input", "Invalid input", { cause, issues });
}

export function actionError(error: unknown): ActionError {
  return error instanceof ActionError
    ? error
    : new ActionError("internal", "Action failed", { cause: error });
}
