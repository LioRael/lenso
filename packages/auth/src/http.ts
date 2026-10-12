import type { Actor, Audience } from "./core";
import { accessOwners } from "./access-owner";
import { interactiveOwners } from "./interactive-owner";
import type { AuthDiscovery, LoginMethod } from "./browser";
import { AuthConfigurationError, AuthError } from "./errors";
import { authErrorResponse, requireSameOrigin } from "./fetch";
import type { InteractiveLogin } from "./interactive";
import type { ManagedSession } from "./sessions";

export interface AuthRequestPolicy {
  readonly origin: string;
  readonly credentialMode: "bearer" | "cookie";
  readonly sessionCookieName?: `__Host-${string}`;
  readonly csrfCookieName?: `__Host-${string}`;
}

export class AuthHttpError extends Error {
  constructor(
    readonly status: 400 | 412,
    readonly code: "bad_request" | "session_changed",
  ) {
    super(code === "bad_request" ? "Invalid authentication request" : "Session changed");
  }
}

function hasControlCharacter(value: string, ceiling = 0x1f) {
  return [...value].some((character) => {
    const code = character.codePointAt(0)!;
    return code <= ceiling || code === 0x7f;
  });
}

function single(request: Request, name: string): string | null {
  const value = request.headers.get(name);
  if (value !== null && (!value || value.includes(",") || hasControlCharacter(value)))
    throw new AuthHttpError(400, "bad_request");
  return value;
}

export function authCookie(request: Request, name: string): string | null {
  const entries = (request.headers.get("cookie") ?? "")
    .split(";")
    .map((part) => part.trim())
    .filter((part) => part.split("=")[0] === name);
  if (entries.length > 1) throw new AuthHttpError(400, "bad_request");
  if (!entries.length) return null;
  const entry = entries[0]!;
  const value = entry.slice(name.length + 1);
  if (
    !entry.startsWith(`${name}=`) ||
    !value ||
    /[\s,;]/u.test(value) ||
    hasControlCharacter(value)
  )
    throw new AuthHttpError(400, "bad_request");
  return value;
}

export function expectedAuthSubject(request: Request): string | null {
  const value = single(request, "x-lenso-expected-subject");
  if (value !== null && (!value.trim() || value.length > 256))
    throw new AuthHttpError(400, "bad_request");
  return value;
}

export function authCookieNames(policy: AuthRequestPolicy) {
  const session = policy.sessionCookieName ?? "__Host-lenso-session";
  const csrf = policy.csrfCookieName ?? "__Host-lenso-csrf";
  if (
    !["cookie", "bearer"].includes(policy.credentialMode) ||
    ![session, csrf].every((name) => /^__Host-[A-Za-z0-9_-]+$/.test(name)) ||
    session === csrf
  )
    throw new AuthConfigurationError(
      "Explicit credential selection and distinct __Host cookies required",
    );
  return { session, csrf };
}

export function requireAuthRequest(request: Request, policy: AuthRequestPolicy): void {
  request.signal.throwIfAborted();
  const origin = new URL(policy.origin);
  if (origin.origin !== policy.origin || !["http:", "https:"].includes(origin.protocol))
    throw new AuthConfigurationError("Exact HTTP origin required");
  const host = single(request, "host");
  const suppliedOrigin = single(request, "origin");
  if (
    new URL(request.url).origin !== origin.origin ||
    (host !== null && host !== origin.host) ||
    (suppliedOrigin !== null && suppliedOrigin !== origin.origin)
  )
    throw new AuthError("FORBIDDEN");
  const names = authCookieNames(policy);
  const credential = authCookie(request, names.session);
  const csrf = authCookie(request, names.csrf);
  const authorization = single(request, "authorization");
  const csrfHeader = single(request, "x-csrf-token");
  expectedAuthSubject(request);
  if (authorization !== null && !/^Bearer [^\s,]+$/i.test(authorization))
    throw new AuthError("UNAUTHORIZED");
  if (
    (authorization !== null && credential !== null) ||
    (policy.credentialMode === "cookie" && authorization !== null) ||
    (policy.credentialMode === "bearer" && credential !== null)
  )
    throw new AuthHttpError(400, "bad_request");
  if (policy.credentialMode === "cookie" && !["GET", "HEAD", "OPTIONS"].includes(request.method)) {
    requireSameOrigin(request, policy.origin);
    if (csrf === null || csrfHeader === null || csrf !== csrfHeader)
      throw new AuthError("FORBIDDEN");
  }
}

function localPath(path: string): string {
  if (
    !/^\/(?:[A-Za-z0-9._~-]+\/)*[A-Za-z0-9._~-]+$/u.test(path) ||
    path.split("/").some((segment) => segment === "." || segment === "..")
  )
    throw new AuthConfigurationError("Auth mounts require exact local paths");
  return path;
}

export function safeAuthReturnPath(value: string | null, roots: readonly string[]): string {
  if (!roots.length) throw new AuthConfigurationError("Approved return roots required");
  for (const root of roots) if (root !== "/") localPath(root);
  const fallback = roots[0]!;
  if (
    !value ||
    !value.startsWith("/") ||
    value.startsWith("//") ||
    value.includes("\\") ||
    hasControlCharacter(value, 0x20) ||
    /%(?:25|2f|5c|0[0-9a-f]|1[0-9a-f]|7f)/iu.test(value)
  )
    return fallback;
  const url = new URL(value, "https://auth.invalid");
  if (
    `${url.pathname}${url.search}${url.hash}` !== value ||
    !roots.some(
      (root) => url.pathname === root || (root !== "/" && url.pathname.startsWith(`${root}/`)),
    )
  )
    return fallback;
  return value;
}

function cookie(name: string, value: string, httpOnly: boolean, maxAge?: number) {
  if (!/^[A-Za-z0-9._-]*$/u.test(value)) throw new AuthError("SERVICE_UNAVAILABLE");
  return (
    `${name}=${value}; Path=/; Secure; SameSite=Strict${httpOnly ? "; HttpOnly" : ""}` +
    (maxAge === undefined ? "" : `; Max-Age=${maxAge}`)
  );
}

export interface AuthHttpBinding<
  R extends string = string,
  E = unknown,
  A extends string = string,
> {
  readonly realmId: R;
  readonly audience: Audience<A>;
  readonly requestPolicy: AuthRequestPolicy;
  readonly methodsPath: string;
  readonly routes: readonly { readonly path: string; readonly method: string }[];
  assertAccess(access: object): void;
  evidence(request: Request): E | Promise<E>;
  hasCredentials(request: Request): boolean;
  admit(request: Request): void;
  fetch(request: Request): Promise<Response | undefined>;
}

/** Adapt the existing external session owner, never create a second session store. */
export interface ExternalSessionActions {
  readonly logout?: { readonly path: string; handle(request: Request): Promise<Response> };
  readonly renew?: { readonly path: string; handle(request: Request): Promise<Response> };
}

export function createAuthHttpBinding<
  R extends string,
  E,
  S extends string,
  A extends string,
>(options: {
  readonly access: {
    readonly realmId: R;
    readonly audience: Audience<A>;
    required(evidence: E, options: { signal: AbortSignal }): Promise<Actor<R, S, A>>;
  };
  readonly requestPolicy: AuthRequestPolicy;
  readonly evidence: (request: Request) => E | Promise<E>;
  readonly hasCredentials?: (request: Request) => boolean;
  readonly methodsPath: string;
  readonly login?: readonly InteractiveLogin[];
  readonly returnRoots: readonly string[];
  readonly sessions?: {
    readonly source: { readonly realmId?: string };
    renew(credential: string, options: { signal: AbortSignal }): Promise<ManagedSession>;
    revoke(credential: string, options: { signal: AbortSignal }): Promise<void>;
  };
  readonly logoutPath?: string;
  readonly renewPath?: string;
  readonly externalSession?: ExternalSessionActions;
}): AuthHttpBinding<R, E, A> {
  const policy = Object.freeze({ ...options.requestPolicy });
  const owner = accessOwners.get(options.access);
  if (!owner)
    throw new AuthConfigurationError("Auth HTTP binding requires an Auth Access instance");
  if (options.sessions && options.sessions.source.realmId !== options.access.realmId)
    throw new AuthConfigurationError("Managed sessions and HTTP Access require the same realm");
  if (options.sessions && options.sessions.source !== owner.source)
    throw new AuthConfigurationError(
      "Managed sessions and HTTP Access require the same session source",
    );
  const names = authCookieNames(policy);
  const origin = new URL(policy.origin);
  if (origin.origin !== policy.origin || !["http:", "https:"].includes(origin.protocol))
    throw new AuthConfigurationError("Exact HTTP origin required");
  const methodsPath = localPath(options.methodsPath);
  const roots = [...options.returnRoots];
  safeAuthReturnPath(null, roots);
  const logins = (options.login ?? []).map((login) => {
    const sessionOwner = interactiveOwners.get(login);
    if (sessionOwner && sessionOwner !== options.sessions)
      throw new AuthConfigurationError(
        "Password login and HTTP session operations require the same session owner",
      );
    const d = login.descriptor;
    if (
      !d.id?.trim() ||
      !d.label?.trim() ||
      !["password", "redirect", "custom"].includes(d.kind) ||
      (d.kind === "custom" && !d.renderer?.trim()) ||
      login.method !== (d.kind === "redirect" ? "GET" : "POST")
    )
      throw new AuthConfigurationError("Invalid interactive capability");
    // Whitelist fields rather than spreading adapter-private data into discovery.
    const descriptor: LoginMethod = Object.freeze({
      id: d.id,
      kind: d.kind,
      label: d.label,
      action: localPath(d.action),
      ...(d.kind === "custom" ? { renderer: d.renderer } : {}),
    }) as LoginMethod;
    if (login.callback && !["GET", "POST"].includes(login.callback.method))
      throw new AuthConfigurationError("Invalid external callback method");
    return {
      ...login,
      descriptor,
      ...(login.callback
        ? {
            callback: Object.freeze({ ...login.callback, path: localPath(login.callback.path) }),
          }
        : {}),
    };
  });
  if (new Set(logins.map((login) => login.descriptor.id)).size !== logins.length)
    throw new AuthConfigurationError("Duplicate login capability");
  if (options.externalSession && (options.sessions || options.logoutPath || options.renewPath))
    throw new AuthConfigurationError("Select one session owner");
  const logoutPath = localOptional(options.externalSession?.logout?.path ?? options.logoutPath);
  const renewPath = localOptional(options.externalSession?.renew?.path ?? options.renewPath);
  if (
    (options.logoutPath || options.renewPath) &&
    (!options.sessions || policy.credentialMode !== "cookie")
  )
    throw new AuthConfigurationError(
      "Managed cookie session operations require their session owner",
    );
  const routes = Object.freeze(
    [
      { path: methodsPath, method: "GET" },
      ...logins.map((login) => ({ path: login.descriptor.action, method: login.method })),
      ...logins.flatMap((login) =>
        login.callback
          ? [
              {
                path: login.callback.path,
                method: login.callback.method,
              },
            ]
          : [],
      ),
      ...(logoutPath ? [{ path: logoutPath, method: "POST" }] : []),
      ...(renewPath ? [{ path: renewPath, method: "POST" }] : []),
    ].map((route) => Object.freeze(route)),
  );
  assertAuthHttpBindings([{ routes, requestPolicy: policy }]);
  const discovery: AuthDiscovery = {
    methods: logins.map((login) => login.descriptor),
    ...(logoutPath || renewPath
      ? {
          session: {
            ...(logoutPath ? { logout: logoutPath } : {}),
            ...(renewPath ? { renew: renewPath } : {}),
          },
        }
      : {}),
    ...(policy.credentialMode === "cookie"
      ? {
          csrf: { cookie_name: names.csrf, header_name: "x-csrf-token" },
        }
      : {}),
  };
  const evidence = options.evidence;
  async function authenticate(request: Request) {
    const actor = await options.access.required(await evidence(request), {
      signal: request.signal,
    });
    const expected = expectedAuthSubject(request);
    if (expected !== null && expected !== actor.subjectId)
      throw new AuthHttpError(412, "session_changed");
    return actor;
  }
  function sessionResponse(session: ManagedSession): Response {
    if (policy.credentialMode !== "cookie") throw new AuthError("SERVICE_UNAVAILABLE");
    return new Response(null, {
      status: 204,
      headers: {
        "set-cookie": cookie(
          names.session,
          session.credential,
          true,
          Math.max(0, Math.floor((session.expiresAt - Date.now()) / 1000)),
        ),
        "cache-control": "no-store",
      },
    });
  }
  return Object.freeze({
    realmId: options.access.realmId,
    audience: options.access.audience,
    requestPolicy: policy,
    methodsPath,
    routes,
    assertAccess(access: object) {
      const selected = access as { realmId?: string; audience?: { id?: string } };
      if (
        accessOwners.get(access) !== owner ||
        selected.realmId !== options.access.realmId ||
        selected.audience?.id !== options.access.audience.id
      )
        throw new AuthConfigurationError(
          "Auth HTTP binding and Access must select the same instance and audience",
        );
    },
    evidence,
    hasCredentials:
      options.hasCredentials ??
      ((request: Request) =>
        policy.credentialMode === "cookie"
          ? authCookie(request, names.session) !== null
          : request.headers.has("authorization")),
    admit: (request: Request) => requireAuthRequest(request, policy),
    async fetch(request: Request) {
      const path = new URL(request.url).pathname;
      if (!routes.some((route) => route.path === path)) return undefined;
      try {
        requireAuthRequest(request, policy);
        if (!routes.some((route) => route.path === path && route.method === request.method))
          return new Response(null, { status: 405, headers: { "cache-control": "no-store" } });
        if (path === methodsPath) {
          const headers = new Headers({ "cache-control": "no-store" });
          if (policy.credentialMode === "cookie" && !authCookie(request, names.csrf)) {
            const secret = Array.from(crypto.getRandomValues(new Uint8Array(32)), (b) =>
              b.toString(16).padStart(2, "0"),
            ).join("");
            headers.append("set-cookie", cookie(names.csrf, secret, false));
          }
          return Response.json(discovery, { headers });
        }
        // Every auth mutation needs Origin, even a bearer-configured external entry.
        if (request.method === "POST") requireSameOrigin(request, policy.origin);
        if (path === logoutPath || path === renewPath || expectedAuthSubject(request) !== null)
          await authenticate(request);
        if (path === logoutPath || path === renewPath) {
          const external =
            path === logoutPath ? options.externalSession?.logout : options.externalSession?.renew;
          if (external) {
            const result = await external.handle(request);
            request.signal.throwIfAborted();
            return privateResponse(result);
          }
          const credential = authCookie(request, names.session);
          if (!credential) throw new AuthError("UNAUTHORIZED");
          if (path === renewPath)
            return sessionResponse(
              await options.sessions!.renew(credential, { signal: request.signal }),
            );
          await options.sessions!.revoke(credential, { signal: request.signal });
          request.signal.throwIfAborted();
          return new Response(null, {
            status: 204,
            headers: {
              "set-cookie": cookie(names.session, "", true, 0),
              "cache-control": "no-store",
            },
          });
        }
        const login = logins.find(
          (entry) => entry.descriptor.action === path || entry.callback?.path === path,
        )!;
        const context = {
          signal: request.signal,
          returnPath: safeAuthReturnPath(new URL(request.url).searchParams.get("return_to"), roots),
        };
        const result =
          login.callback?.path === path
            ? await login.callback.complete(request, context)
            : await login.start(request, context);
        request.signal.throwIfAborted();
        if (result instanceof Response) {
          return privateResponse(result);
        }
        const previous = authCookie(request, names.session);
        if (previous && options.sessions) {
          try {
            await options.sessions.revoke(previous, { signal: request.signal });
          } catch (error) {
            request.signal.throwIfAborted();
            if (!(error instanceof AuthError) || error.code !== "UNAUTHORIZED") throw error;
          }
        }
        return sessionResponse(result);
      } catch (error) {
        request.signal.throwIfAborted();
        const safe =
          error instanceof AuthHttpError
            ? error.code === "session_changed"
              ? new AuthHttpError(412, "session_changed")
              : new AuthHttpError(400, "bad_request")
            : undefined;
        const response = safe
          ? Response.json({ code: safe.code, message: safe.message }, { status: safe.status })
          : authErrorResponse(error);
        response.headers.set("cache-control", "no-store");
        return response;
      }
    },
  });
}

function localOptional(path: string | undefined) {
  return path === undefined ? undefined : localPath(path);
}

function privateResponse(result: Response) {
  // Fetch-created/redirect responses can have immutable headers.
  const response = new Response(result.body, {
    status: result.status,
    statusText: result.statusText,
    headers: result.headers,
  });
  response.headers.set("cache-control", "no-store");
  return response;
}

/** Validate the concrete bindings the host will compose, plus its reserved routes. */
export function assertAuthHttpBindings(
  bindings: readonly Pick<AuthHttpBinding, "routes" | "requestPolicy">[],
  reserved: readonly { readonly path: string; readonly method: string }[] = [],
): void {
  // A binding owns its paths, including 405 responses. Distinct methods on
  // separate bindings therefore cannot share a path on the same origin.
  const reservedPaths = new Set(reserved.map((route) => localPath(route.path)));
  const routes = new Set<string>();
  const cookies = new Set<string>();
  for (const binding of bindings) {
    if (binding.requestPolicy.credentialMode === "cookie") {
      for (const name of Object.values(authCookieNames(binding.requestPolicy))) {
        const key = `${binding.requestPolicy.origin} ${name}`;
        if (cookies.has(key))
          throw new AuthConfigurationError("Auth bindings share a cookie namespace");
        cookies.add(key);
      }
    }
    for (const route of binding.routes) {
      const path = localPath(route.path);
      const key = `${binding.requestPolicy.origin} ${path}`;
      if (reservedPaths.has(path) || routes.has(key))
        throw new AuthConfigurationError("Duplicate auth mount");
      routes.add(key);
    }
  }
}
