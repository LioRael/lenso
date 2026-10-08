import { AuthConfigurationError, AuthError } from "./errors";
import type { SessionEvidence, SourceCapabilities } from "./source";

export interface SessionRequirements {
  readonly authoritative?: boolean;
  readonly maxSessionAgeMs?: number;
  readonly maxAuthenticationAgeMs?: number;
  readonly assurance?: readonly string[];
}

function age(value: number): number {
  if (!Number.isSafeInteger(value) || value <= 0) {
    throw new AuthConfigurationError("Authentication age must be positive milliseconds");
  }
  return value;
}

export function authoritativeSession(): SessionRequirements {
  return Object.freeze({ authoritative: true });
}

export function sessionCreatedWithin(milliseconds: number): SessionRequirements {
  return Object.freeze({ maxSessionAgeMs: age(milliseconds) });
}

export function authenticatedWithin(milliseconds: number): SessionRequirements {
  return Object.freeze({ maxAuthenticationAgeMs: age(milliseconds) });
}

export function requireAssurance(value: string): SessionRequirements {
  if (!value.trim() || value.length > 128) {
    throw new AuthConfigurationError("Assurance must be a nonempty identifier");
  }
  return Object.freeze({ assurance: Object.freeze([value]) });
}

export function allRequirements(
  ...requirements: readonly SessionRequirements[]
): SessionRequirements {
  let authoritative = false;
  let sessionAge: number | undefined;
  let authenticationAge: number | undefined;
  const assurance = new Set<string>();
  for (const requirement of requirements) {
    if (
      !requirement ||
      (requirement.authoritative !== undefined && typeof requirement.authoritative !== "boolean") ||
      (requirement.assurance !== undefined && !Array.isArray(requirement.assurance))
    ) {
      throw new AuthConfigurationError("Malformed authentication requirements");
    }
    authoritative ||= requirement.authoritative === true;
    if (requirement.maxSessionAgeMs !== undefined) {
      sessionAge = Math.min(sessionAge ?? Infinity, age(requirement.maxSessionAgeMs));
    }
    if (requirement.maxAuthenticationAgeMs !== undefined) {
      authenticationAge = Math.min(
        authenticationAge ?? Infinity,
        age(requirement.maxAuthenticationAgeMs),
      );
    }
    for (const value of requirement.assurance ?? []) {
      requireAssurance(value);
      assurance.add(value);
    }
  }
  return Object.freeze({
    ...(authoritative ? { authoritative: true } : {}),
    ...(sessionAge === undefined ? {} : { maxSessionAgeMs: sessionAge }),
    ...(authenticationAge === undefined ? {} : { maxAuthenticationAgeMs: authenticationAge }),
    ...(assurance.size === 0 ? {} : { assurance: Object.freeze([...assurance]) }),
  });
}

export function checkCapabilities(
  requirements: SessionRequirements,
  capabilities: SourceCapabilities,
): void {
  if (
    (requirements.authoritative && capabilities.authoritative !== true) ||
    (requirements.maxSessionAgeMs !== undefined && capabilities.sessionCreatedAt !== true) ||
    (requirements.maxAuthenticationAgeMs !== undefined && capabilities.authenticatedAt !== true) ||
    requirements.assurance?.some((value) => !capabilities.assurance?.includes(value))
  ) {
    throw new AuthConfigurationError("Source cannot satisfy the authentication requirements");
  }
}

export function checkSession(
  requirements: SessionRequirements,
  session: SessionEvidence | undefined,
  now: number,
): void {
  if (requirements.authoritative && session?.authoritative !== true) {
    throw new AuthError("REAUTHENTICATION_REQUIRED");
  }
  if (
    requirements.maxSessionAgeMs !== undefined &&
    (session?.sessionCreatedAt === undefined ||
      now - session.sessionCreatedAt >= requirements.maxSessionAgeMs)
  ) {
    throw new AuthError("REAUTHENTICATION_REQUIRED");
  }
  if (
    requirements.maxAuthenticationAgeMs !== undefined &&
    (session?.authenticatedAt === undefined ||
      now - session.authenticatedAt >= requirements.maxAuthenticationAgeMs)
  ) {
    throw new AuthError("REAUTHENTICATION_REQUIRED");
  }
  if (requirements.assurance?.some((value) => !session?.assurance?.includes(value))) {
    throw new AuthError("REAUTHENTICATION_REQUIRED");
  }
}
