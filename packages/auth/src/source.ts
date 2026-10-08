export type ActorKind = "user" | "guest" | "service";

/** Epoch milliseconds. Session creation is not evidence of reauthentication. */
export interface SessionEvidence {
  readonly expiresAt: number;
  readonly sessionCreatedAt?: number;
  readonly authenticatedAt?: number;
  readonly assurance?: readonly string[];
  readonly authoritative?: boolean;
}

export interface SourceCapabilities {
  readonly authoritative?: boolean;
  readonly sessionCreatedAt?: boolean;
  readonly authenticatedAt?: boolean;
  readonly assurance?: readonly string[];
}

export interface VerificationContext {
  readonly signal: AbortSignal;
  /** Sources advertising this capability must bypass their caches when requested. */
  readonly authoritative?: boolean;
}

export type AuthenticationResult<S extends string = string> =
  | { readonly status: "absent" }
  | { readonly status: "rejected" }
  | { readonly status: "unresolved" }
  | {
      readonly status: "verified";
      readonly subjectId: S;
      readonly kind?: ActorKind;
      readonly session?: SessionEvidence;
    };

/** Installed sources are trusted code; they must verify, not decode, credentials. */
export interface AuthSource<E, S extends string = string> {
  readonly realmId?: string;
  readonly capabilities?: SourceCapabilities;
  verify(evidence: E, context: VerificationContext): Promise<AuthenticationResult<S>>;
}

export function defineSource<E, S extends string = string>(
  source: AuthSource<E, S>,
): AuthSource<E, S> {
  return source;
}

/** Detach verified facts before any other callback can mutate source-owned objects. */
export function snapshotVerification<S extends string>(
  value: Extract<AuthenticationResult<S>, { status: "verified" }>,
): Extract<AuthenticationResult<S>, { status: "verified" }> {
  const session = value.session;
  return Object.freeze({
    status: "verified",
    subjectId: value.subjectId,
    kind: value.kind,
    ...(session === undefined
      ? {}
      : {
          session:
            session &&
            Object.freeze({
              expiresAt: session.expiresAt,
              sessionCreatedAt: session.sessionCreatedAt,
              authenticatedAt: session.authenticatedAt,
              authoritative: session.authoritative,
              assurance: Array.isArray(session.assurance)
                ? Object.freeze([...session.assurance])
                : session.assurance,
            }),
        }),
  });
}
