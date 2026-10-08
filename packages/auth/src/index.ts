export {
  audience,
  realm,
  createAuth,
  allPolicies,
  type Actor,
  type ActorOf,
  type SubjectRef,
  type Audience,
  type Realm,
  type Access,
  type Auth,
  type AuthenticationOptions,
  type MembershipReader,
  type Policy,
  type PolicyContext,
} from "./core";
export { AuthError, AuthConfigurationError, type AuthErrorCode } from "./errors";
export {
  defineSource,
  type AuthSource,
  type ActorKind,
  type AuthenticationResult,
  type SessionEvidence,
  type SourceCapabilities,
  type VerificationContext,
} from "./source";
export {
  authoritativeSession,
  sessionCreatedWithin,
  authenticatedWithin,
  requireAssurance,
  allRequirements,
  type SessionRequirements,
} from "./requirements";
