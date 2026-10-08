import { ORPCError, os } from "@orpc/server";
import { definePlugin } from "lenso";

/** A projected identity only; never return a session, token or provider credentials. */
export interface Identity {
  readonly subject: string;
  readonly tenantId?: string;
}
export type Authentication =
  | { status: "anonymous" }
  | { status: "invalid" }
  | { status: "authenticated"; identity: Identity };
export interface AuthContext {
  request: Request;
  signal?: AbortSignal;
}
export interface AuthProvider {
  authenticate(context: AuthContext): Promise<Authentication>;
}

/** Safe messages also apply to direct service callers, not just the transport. */
export class AuthError extends Error {
  constructor(readonly code: "UNAUTHORIZED" | "FORBIDDEN" | "SERVICE_UNAVAILABLE") {
    super(
      code === "UNAUTHORIZED"
        ? "Authentication required"
        : code === "FORBIDDEN"
          ? "Access denied"
          : "Authentication unavailable",
    );
    this.name = "AuthError";
  }
}

export function requireIdentity(identity: Identity | null): Identity {
  if (!identity) throw new AuthError("UNAUTHORIZED");
  return identity;
}

/** Call inside the business service after loading trusted tenant/object ownership. */
export async function authorize(
  identity: Identity | null,
  policy: (identity: Identity) => boolean | Promise<boolean>,
): Promise<Identity> {
  const authenticated = requireIdentity(identity);
  let allowed: boolean;
  try {
    allowed = await policy(authenticated);
  } catch {
    throw new AuthError("SERVICE_UNAVAILABLE");
  }
  if (allowed !== true) throw new AuthError("FORBIDDEN");
  return authenticated;
}

export interface AuthService {
  authenticate(context: AuthContext): Promise<Identity | null>;
}

export function createAuthPlugin(options: { id?: string; provider: AuthProvider }) {
  return definePlugin<AuthService>({
    id: options.id ?? "auth",
    setup() {
      const requests = new WeakMap<Request, Promise<Identity | null>>();
      return {
        authenticate(context) {
          const cached = requests.get(context.request);
          if (cached) return cached;
          const result = (async () => {
            const signal = context.signal ?? context.request.signal;
            signal.throwIfAborted();
            let authentication: Authentication;
            try {
              authentication = await options.provider.authenticate({ ...context, signal });
            } catch {
              signal.throwIfAborted();
              throw new AuthError("SERVICE_UNAVAILABLE");
            }
            signal.throwIfAborted();
            if (authentication?.status === "anonymous") return null;
            if (authentication?.status !== "authenticated") throw new AuthError("UNAUTHORIZED");
            const identity = authentication.identity;
            if (
              !identity ||
              typeof identity.subject !== "string" ||
              !identity.subject.trim() ||
              (identity.tenantId !== undefined &&
                (typeof identity.tenantId !== "string" || !identity.tenantId.trim()))
            ) {
              throw new AuthError("UNAUTHORIZED");
            }
            return Object.freeze({
              subject: identity.subject,
              ...(identity.tenantId === undefined ? {} : { tenantId: identity.tenantId }),
            });
          })();
          requests.set(context.request, result);
          return result;
        },
      };
    },
  });
}

/** Compatible with Better Auth's documented auth.api.getSession({ headers }). */
export function createSessionProvider<Session>(options: {
  getSession(input: { headers: Headers }): Promise<Session | null>;
  identity(session: Session): Identity;
}): AuthProvider {
  return {
    async authenticate({ request, signal }) {
      signal?.throwIfAborted();
      const session = await options.getSession({ headers: request.headers });
      signal?.throwIfAborted();
      return session === null
        ? { status: "anonymous" }
        : { status: "authenticated", identity: options.identity(session) };
    },
  };
}

function transportError(error: unknown): never {
  if (error instanceof AuthError) throw new ORPCError(error.code, { message: error.message });
  throw new ORPCError("SERVICE_UNAVAILABLE", { message: "Authentication unavailable" });
}

/** Explicit opt-in anonymous access. Invalid identities still fail closed. */
export function optionalAuth(auth: AuthService) {
  return os.$context<AuthContext>().middleware(async ({ context, next }) => {
    let identity: Identity | null;
    try {
      identity = await auth.authenticate(context);
    } catch (error) {
      transportError(error);
    }
    try {
      return await next({ context: { identity } });
    } catch (error) {
      if (error instanceof AuthError) transportError(error);
      throw error;
    }
  });
}

/** Protected middleware is the default; identity is typed as non-null afterward. */
export function requiredAuth(auth: AuthService) {
  return os.$context<AuthContext>().middleware(async ({ context, next }) => {
    let identity: Identity;
    try {
      identity = requireIdentity(await auth.authenticate(context));
    } catch (error) {
      transportError(error);
    }
    try {
      return await next({ context: { identity } });
    } catch (error) {
      if (error instanceof AuthError) transportError(error);
      throw error;
    }
  });
}
