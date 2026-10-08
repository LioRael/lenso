import { AuthError, type Access, type Actor, type PolicyContext } from "@lenso/auth";
import { AuthorizationError } from "./errors";
import type {
  Attributes,
  Awaitable,
  Authorization,
  CredentialLimit,
  Decision,
  Resource,
} from "./types";

export interface AuthorizationFacts<A extends string, R extends Resource, C> {
  readonly resource: R;
  readonly context: C;
  readonly attributes?: Attributes;
  /** Independently verified credential ceiling. Never read from business JSON. */
  readonly credential?: CredentialLimit<A>;
}

export interface AuthorizedAccess<P, A extends string, T> {
  check(
    actor: P | null,
    action: A,
    resource: T,
    options?: { signal?: AbortSignal },
  ): Promise<Decision>;
  can(
    actor: P | null,
    action: A,
    resource: T,
    options?: { signal?: AbortSignal },
  ): Promise<boolean>;
  enforce(
    actor: P | null,
    action: A,
    resource: T,
    options?: { signal?: AbortSignal },
  ): Promise<void>;
}

/** Every call reuses the exact Access.enforce chain; this is not another authenticator. */
export function createAuthorizedAccess<
  Realm extends string,
  E,
  S extends string,
  Audience extends string,
  T,
  M,
  A extends string,
  R extends Resource,
  C,
>(
  access: Access<Realm, E, S, Audience, T, M>,
  authorization: Authorization<A, R, C>,
  facts: (
    verified: PolicyContext<Actor<Realm, S, Audience>, T, M>,
  ) => Awaitable<AuthorizationFacts<A, R, C>>,
): AuthorizedAccess<Actor<Realm, S, Audience>, A, T> {
  const check: AuthorizedAccess<Actor<Realm, S, Audience>, A, T>["check"] = async (
    actor,
    action,
    resource,
    options,
  ) => {
    let decision: Decision | undefined;
    try {
      // Keep membership lookup and policy projection on the same detached business data.
      // Cloneable records may include Date; clients, functions and resource handles are refused.
      const invocationResource = structuredClone(resource);
      await access.enforce(
        actor,
        invocationResource,
        async (verified) => {
          const trusted = await facts(
            Object.freeze({
              ...verified,
              membership: structuredClone(verified.membership),
            }),
          );
          decision = await authorization.check(
            {
              principal: {
                realmId: verified.principal.realmId,
                subjectId: verified.principal.subjectId,
                kind: verified.principal.kind,
                ...(trusted.attributes ? { attributes: trusted.attributes } : {}),
              },
              action,
              resource: trusted.resource,
              context: trusted.context,
              audience: verified.principal.audience,
              ...(trusted.credential ? { credential: trusted.credential } : {}),
            },
            { signal: verified.signal },
          );
          return decision.effect === "allow";
        },
        options,
      );
      return decision ?? Object.freeze({ effect: "deny", code: "EVALUATION_FAILED" });
    } catch (error) {
      if (error instanceof AuthError) {
        if (error.code === "FORBIDDEN" && decision?.effect === "deny") return decision;
        if (error.code === "UNAUTHORIZED" || error.code === "REAUTHENTICATION_REQUIRED")
          return Object.freeze({ effect: "deny", code: "BOUNDARY_DENIED" });
      }
      return Object.freeze({
        effect: "deny",
        code: options?.signal?.aborted ? "CANCELLED" : "EVALUATION_FAILED",
      });
    }
  };
  return Object.freeze({
    check,
    async can(...args: Parameters<typeof check>) {
      return (await check(...args)).effect === "allow";
    },
    async enforce(...args: Parameters<typeof check>) {
      if ((await check(...args)).effect !== "allow") throw new AuthorizationError();
    },
  });
}
