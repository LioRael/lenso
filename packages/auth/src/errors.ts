export type AuthErrorCode =
  | "UNAUTHORIZED"
  | "FORBIDDEN"
  | "SERVICE_UNAVAILABLE"
  | "REAUTHENTICATION_REQUIRED";

const messages: Record<AuthErrorCode, string> = {
  UNAUTHORIZED: "Authentication required",
  FORBIDDEN: "Access denied",
  SERVICE_UNAVAILABLE: "Authentication unavailable",
  REAUTHENTICATION_REQUIRED: "Reauthentication required",
};

export class AuthError extends Error {
  readonly code: AuthErrorCode;

  constructor(code: AuthErrorCode, options?: ErrorOptions) {
    const safeCode =
      typeof code === "string" && Object.hasOwn(messages, code) ? code : "SERVICE_UNAVAILABLE";
    super(messages[safeCode], options);
    this.code = safeCode;
    this.name = "AuthError";
  }
}

/** Transport-neutral projection; neither caller input nor mutable Error text is public. */
export function authErrorDiagnostic(error: unknown) {
  if (!(error instanceof AuthError)) return undefined;
  const safe = new AuthError(error.code);
  return { code: safe.code, phase: "invoke", message: safe.message } as const;
}

export class AuthConfigurationError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "AuthConfigurationError";
  }
}
