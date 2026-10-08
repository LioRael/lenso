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

  constructor(code: AuthErrorCode) {
    const safeCode =
      typeof code === "string" && Object.hasOwn(messages, code) ? code : "SERVICE_UNAVAILABLE";
    super(messages[safeCode]);
    this.code = safeCode;
    this.name = "AuthError";
  }
}

export class AuthConfigurationError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "AuthConfigurationError";
  }
}
