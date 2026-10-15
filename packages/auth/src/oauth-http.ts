import { AuthError } from "./errors";
import { OAuthError, type OAuthAuthorizationServer } from "./oauth";
import { requireSameOrigin } from "./fetch";

const security = {
  "cache-control": "no-store",
  "referrer-policy": "no-referrer",
  "x-content-type-options": "nosniff",
};
async function body(request: Request, type: string) {
  if (request.headers.get("content-type")?.split(";")[0]?.trim() !== type || !request.body)
    throw new OAuthError("invalid_request");
  const reader = request.body.getReader();
  let size = 0,
    text = "";
  const decoder = new TextDecoder("utf-8", { fatal: true });
  try {
    for (;;) {
      const part = await reader.read();
      if (part.done) break;
      size += part.value.byteLength;
      if (size > 8192) throw new OAuthError("invalid_request");
      text += decoder.decode(part.value, { stream: true });
    }
    return text + decoder.decode();
  } finally {
    await reader.cancel().catch(() => {});
    reader.releaseLock();
  }
}

/** Mount into an existing listener. Human evidence and CSRF remain host-owned.
 * Token/registration endpoints never select browser cookies as authentication. */
export function createOAuthHttpBinding<E>(options: {
  server: OAuthAuthorizationServer<E>;
  consentPage: string;
  evidence(request: Request): E | Promise<E>;
  /** Existing cookie boundary must enforce CSRF and identity-transition fencing. */
  admitHuman(request: Request): void | Promise<void>;
  /** Required admission for public OAuth endpoints (durable host rate limits). */
  admitPublic(request: Request): void | Promise<void>;
}) {
  const { server } = options,
    issuer = new URL(server.issuer),
    origin = issuer.origin;
  const consentPage = new URL(options.consentPage, origin);
  if (consentPage.origin !== origin) throw new TypeError("Consent UI must share the issuer origin");
  const base = issuer.pathname.replace(/\/$/, "");
  const metadata = "/.well-known/oauth-authorization-server" + base;
  const json = (value: unknown, status = 200) =>
    Response.json(value, { status, headers: security });
  return {
    async fetch(request: Request): Promise<Response | undefined> {
      const url = new URL(request.url),
        path = url.pathname;
      if (
        path !== metadata &&
        ![
          "authorize",
          "register",
          "token",
          "revoke",
          "consent",
          "decision",
          "grants",
          "grants/revoke",
        ].some((s) => path === base + "/" + s)
      )
        return undefined;
      try {
        if (url.origin !== origin) throw new OAuthError("invalid_request");
        if (path === metadata)
          return request.method === "GET"
            ? json(server.metadata)
            : new Response(null, { status: 405, headers: security });
        if (["authorize", "register", "token", "revoke"].some((s) => path === base + "/" + s))
          await options.admitPublic(request);
        if (path === base + "/authorize") {
          if (request.method !== "GET" || url.search.length > 8192)
            throw new OAuthError("invalid_request");
          const pending = await server.begin(url.searchParams, request.signal);
          const next = new URL(consentPage);
          next.searchParams.set("request_id", pending.requestId);
          return new Response(null, { status: 303, headers: { ...security, location: next.href } });
        }
        if (path === base + "/register") {
          if (
            request.method !== "POST" ||
            request.headers.has("authorization") ||
            request.headers.has("cookie")
          )
            throw new OAuthError("invalid_request");
          return json(
            await server.register(
              JSON.parse(await body(request, "application/json")),
              request.signal,
            ),
            201,
          );
        }
        if (path === base + "/token" || path === base + "/revoke") {
          if (
            request.method !== "POST" ||
            request.headers.has("authorization") ||
            request.headers.has("cookie")
          )
            throw new OAuthError("invalid_request");
          const p = new URLSearchParams(await body(request, "application/x-www-form-urlencoded"));
          if (path.endsWith("/token")) return json(await server.exchange(p, request.signal));
          for (const key of p.keys())
            if (p.getAll(key).length !== 1) throw new OAuthError("invalid_request");
          await server.revokeToken(p.get("client_id") ?? "", p.get("token") ?? "", request.signal);
          return json({});
        }
        const read = path === base + "/consent" || path === base + "/grants";
        if (request.method !== (read ? "GET" : "POST"))
          return new Response(null, { status: 405, headers: security });
        if (!read) requireSameOrigin(request, origin);
        else if (
          (request.headers.get("origin") && request.headers.get("origin") !== origin) ||
          request.headers.get("sec-fetch-site") === "cross-site"
        )
          throw new AuthError("FORBIDDEN");
        await options.admitHuman(request);
        const evidence = await options.evidence(request);
        if (path.endsWith("/consent")) {
          if (url.searchParams.getAll("request_id").length !== 1)
            throw new OAuthError("invalid_request");
          const result = await server.consent(
            url.searchParams.get("request_id")!,
            evidence,
            request.signal,
          );
          await options.admitHuman(request);
          return json(result);
        }
        if (path.endsWith("/grants")) {
          const result = await server.grants(evidence, request.signal);
          await options.admitHuman(request);
          return json(result);
        }
        const value = JSON.parse(await body(request, "application/json"));
        if (path.endsWith("/decision")) {
          if (!value || typeof value.requestId !== "string" || typeof value.approve !== "boolean")
            throw new OAuthError("invalid_request");
          const result = await server.decide(
            value.requestId,
            value.approve,
            evidence,
            request.signal,
          );
          await options.admitHuman(request);
          return json(result);
        }
        if (!value || typeof value.id !== "string") throw new OAuthError("invalid_request");
        await server.revokeGrant(value.id, evidence, request.signal);
        await options.admitHuman(request);
        return json({ revoked: true });
      } catch (error) {
        if (error instanceof OAuthError)
          return json({ error: error.code }, error.code === "access_denied" ? 403 : 400);
        if (error instanceof AuthError) {
          if (error.code === "SERVICE_UNAVAILABLE") return json({ error: "server_error" }, 503);
          const login = error.code === "UNAUTHORIZED" || error.code === "REAUTHENTICATION_REQUIRED";
          return json({ error: login ? "login_required" : "access_denied" }, login ? 401 : 403);
        }
        return json({ error: "server_error" }, 503);
      }
    },
  };
}
