import { AuthError } from "./errors";
import type { AuthenticationOptions } from "./core";

export interface FetchAuthContext {
  readonly request: Request;
  readonly signal?: AbortSignal;
}

export interface EvidenceInput<E> extends AuthenticationOptions {
  readonly evidence: E;
}

function requestSignal(context: FetchAuthContext): AbortSignal {
  return context.signal
    ? AbortSignal.any([context.request.signal, context.signal])
    : context.request.signal;
}

export function headersEvidence(context: FetchAuthContext): EvidenceInput<Headers> {
  return {
    evidence: new Headers(context.request.headers),
    signal: requestSignal(context),
  };
}

/** Bearer only. Never fall back to cookies when a selected credential is invalid. */
export function bearerEvidence(context: FetchAuthContext): EvidenceInput<string | null> {
  const value = context.request.headers.get("authorization");
  if (value === null) {
    return { evidence: null, signal: requestSignal(context) };
  }
  const match = /^Bearer ([^\s,]+)$/i.exec(value);
  if (!match) throw new AuthError("UNAUTHORIZED");
  return { evidence: match[1]!, signal: requestSignal(context) };
}

/** Cookie writes need this gate in addition to authentication and object authorization. */
export function requireSameOrigin(request: Request, allowedOrigin: string): void {
  if (["GET", "HEAD", "OPTIONS"].includes(request.method)) {
    throw new AuthError("FORBIDDEN");
  }
  let configured: URL;
  try {
    configured = new URL(allowedOrigin);
  } catch {
    throw new AuthError("FORBIDDEN");
  }
  if (
    configured.origin !== allowedOrigin ||
    request.headers.get("origin") !== configured.origin ||
    request.headers.get("sec-fetch-site") === "cross-site"
  ) {
    throw new AuthError("FORBIDDEN");
  }
}

export function authErrorResponse(error: unknown): Response {
  if (!(error instanceof AuthError)) {
    return Response.json(
      { code: "SERVICE_UNAVAILABLE", message: "Authentication unavailable" },
      {
        status: 503,
      },
    );
  }
  const safe = new AuthError(error.code);
  const status = safe.code === "FORBIDDEN" ? 403 : safe.code === "SERVICE_UNAVAILABLE" ? 503 : 401;
  return Response.json({ code: safe.code, message: safe.message }, { status });
}
