export class AuthorizationError extends Error {
  constructor() {
    super("Access denied.");
    this.name = "AuthorizationError";
  }
}

export class AuthorizationConfigurationError extends Error {
  constructor() {
    super("Invalid authorization configuration.");
    this.name = "AuthorizationConfigurationError";
  }
}
