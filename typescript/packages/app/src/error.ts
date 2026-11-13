/** Stable classifications shared by native execution and external adapters. */
export type ErrorCode =
  | "invalid_input"
  | "unauthenticated"
  | "forbidden"
  | "not_found"
  | "conflict"
  | "cancelled"
  | "internal";

export class ActionError extends Error {
  constructor(
    readonly code: ErrorCode,
    message: string,
    options?: ErrorOptions,
  ) {
    super(message, options);
    this.name = "ActionError";
  }
}

export function actionError(error: unknown): ActionError {
  return error instanceof ActionError
    ? error
    : new ActionError("internal", "Action failed", { cause: error });
}
