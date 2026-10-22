import type { Access, SubjectRef } from "./core";
import { AuthConfigurationError, AuthError } from "./errors";
import { defineSource } from "./source";
import type { OAuthRecord, OAuthStore } from "./oauth-store";
export {
  createSqlOAuthStore,
  type OAuthStore,
  type OAuthRecord,
  type OAuthSqlDriver,
} from "./oauth-store";

export interface OAuthPublicClient {
  id: string;
  name: string;
  redirectUris: readonly string[];
}
export interface OAuthConsent {
  requestId: string;
  client: { id: string; name: string };
  resource: string;
  scopes: readonly string[];
  expiresAt: number;
}
export interface OAuthGrant {
  id: string;
  clientId: string;
  clientName: string;
  subjectId: string;
  resource: string;
  scopes: readonly string[];
  createdAt: number;
  expiresAt: number;
  revoked: boolean;
}
export class OAuthError extends Error {
  constructor(
    readonly code:
      | "invalid_request"
      | "invalid_client"
      | "invalid_grant"
      | "invalid_scope"
      | "invalid_target"
      | "access_denied"
      | "unsupported_grant_type",
  ) {
    super(code);
  }
}
type Pending = {
  clientId: string;
  redirect: string;
  state?: string;
  challenge: string;
  scopes: string[];
  used: boolean;
};
type Code = {
  clientId: string;
  redirect: string;
  challenge: string;
  grantId: string;
  used: boolean;
};
type Grant = OAuthGrant & {
  accessHash: string;
  accessExpiresAt: number;
  refreshHash: string;
  usedRefresh: string[];
};
const tokenPattern = /^(loa|lor)_([a-f0-9-]{36})\.([A-Za-z0-9_-]{43})$/;
function fail(code: OAuthError["code"]): never {
  throw new OAuthError(code);
}
export async function oauthDigest(value: string) {
  return Array.from(
    new Uint8Array(await crypto.subtle.digest("SHA-256", new TextEncoder().encode(value))),
    (x) => x.toString(16).padStart(2, "0"),
  ).join("");
}
function random() {
  return btoa(String.fromCharCode(...crypto.getRandomValues(new Uint8Array(32))))
    .replace(/\+/g, "-")
    .replace(/\//g, "_")
    .replace(/=+$/g, "");
}
async function challenge(value: string) {
  const hash = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(value));
  return btoa(String.fromCharCode(...new Uint8Array(hash)))
    .replace(/\+/g, "-")
    .replace(/\//g, "_")
    .replace(/=+$/g, "");
}
function equal(a: string, b: string) {
  if (a.length !== b.length) return false;
  let difference = 0;
  for (let i = 0; i < a.length; i++) difference |= a.charCodeAt(i) ^ b.charCodeAt(i);
  return difference === 0;
}
function trustedUrl(value: string) {
  const url = new URL(value);
  if (
    url.username ||
    url.password ||
    url.hash ||
    (url.protocol !== "https:" &&
      !(url.protocol === "http:" && ["127.0.0.1", "[::1]"].includes(url.hostname)))
  )
    throw new TypeError("OAuth URLs require HTTPS or literal loopback");
  return url;
}
/** RFC8252 native loopback port may vary; host/path/query still match exactly. */
export function oauthRedirectMatches(registered: string, actual: string) {
  try {
    const a = trustedUrl(registered),
      b = trustedUrl(actual);
    if (a.protocol === "http:" && ["127.0.0.1", "[::1]"].includes(a.hostname)) {
      a.port = "";
      b.port = "";
    }
    return a.href === b.href;
  } catch {
    return false;
  }
}
/** Ordinary Auth service. No listener, browser, database acquisition, or auto registration. */
export function createOAuthAuthorizationServer<E>(options: {
  issuer: string;
  resource: string;
  clients: readonly OAuthPublicClient[];
  scopes: readonly string[];
  store: OAuthStore;
  humanAccess: Access<string, E, string, string>;
  /** Authoritative current membership; null denies disabled/deleted/unassigned identities. */
  allowedScopes(subject: SubjectRef, signal: AbortSignal): Promise<readonly string[] | null>;
  registration?: { enabled: true; maxRedirectUris?: number };
  now?: () => number;
}) {
  const issuer = trustedUrl(options.issuer).href.replace(/\/$/, ""),
    resource = trustedUrl(options.resource).href;
  const clients = new Map(
    options.clients.map((c) => [
      c.id,
      Object.freeze({ ...c, redirectUris: Object.freeze([...c.redirectUris]) }),
    ]),
  );
  if (
    clients.size !== options.clients.length ||
    (!clients.size && !options.registration?.enabled) ||
    options.scopes.some((s) => !s || !/^[\x21\x23-\x5B\x5D-\x7E]+$/.test(s))
  )
    throw new TypeError("Invalid OAuth registration");
  for (const c of clients.values()) {
    if (!c.id || !c.name || !c.redirectUris.length) throw new TypeError("Invalid OAuth client");
    for (const r of c.redirectUris) trustedUrl(r);
  }
  const now = options.now ?? Date.now;
  const record = async <T>(kind: string, id: string, signal: AbortSignal) => {
    signal.throwIfAborted();
    const r = await options.store.read(kind, id);
    signal.throwIfAborted();
    if (!r || r.expiresAt <= now()) fail("invalid_grant");
    return { ...r, value: r.value as T };
  };
  const allowed = async (subjectId: string, signal: AbortSignal) => {
    signal.throwIfAborted();
    const value = await options.allowedScopes(
      { realmId: options.humanAccess.realmId, subjectId },
      signal,
    );
    signal.throwIfAborted();
    if (!value) fail("access_denied");
    return value;
  };
  async function human(evidence: E, signal: AbortSignal) {
    const actor = await options.humanAccess.required(evidence, { signal });
    if (actor.kind !== "user") throw new AuthError("FORBIDDEN");
    return actor;
  }
  async function client(id: string): Promise<OAuthPublicClient> {
    const fixed = clients.get(id);
    if (fixed) return fixed;
    if (!options.registration?.enabled || !/^loc_[a-f0-9-]{36}$/.test(id)) fail("invalid_client");
    const registered = await options.store.read("client", id);
    if (!registered || registered.expiresAt <= now()) fail("invalid_client");
    return registered.value as OAuthPublicClient;
  }
  function publicGrant(g: Grant): OAuthGrant {
    const {
      id,
      clientId,
      clientName,
      subjectId,
      resource: grantResource,
      scopes,
      createdAt,
      expiresAt,
      revoked,
    } = g;
    return {
      id,
      clientId,
      clientName,
      subjectId,
      resource: grantResource,
      scopes: [...scopes],
      createdAt,
      expiresAt,
      revoked,
    };
  }
  async function save(r: OAuthRecord, signal: AbortSignal) {
    signal.throwIfAborted();
    if (!(await options.store.replace(r, r.revision))) fail("invalid_grant");
    signal.throwIfAborted();
  }
  async function saveLive(r: OAuthRecord, signal: AbortSignal) {
    signal.throwIfAborted();
    // Permission IO and store acknowledgement can both cross the deadline.
    // A late acknowledgement must never trigger another, possibly committed CAS.
    if (r.expiresAt <= now()) fail("invalid_grant");
    await save(r, signal);
    if (r.expiresAt <= now()) fail("invalid_grant");
  }
  function redirect(p: Pending, values: Record<string, string>) {
    const url = new URL(p.redirect);
    for (const [k, v] of Object.entries({
      ...values,
      ...(p.state !== undefined ? { state: p.state } : {}),
      iss: issuer,
    }))
      url.searchParams.set(k, v);
    return url.href;
  }
  async function readConsent(requestId: string, evidence: E, signal: AbortSignal) {
    const actor = await human(evidence, signal);
    const r = await record<Pending>("request", await oauthDigest(requestId), signal);
    if (r.value.used || (r.subjectId && r.subjectId !== actor.subjectId)) fail("invalid_grant");
    const permissions = await allowed(actor.subjectId, signal);
    const scopes = r.value.scopes.filter((s) => permissions.includes(s));
    if (!scopes.length) fail("invalid_scope");
    if (!r.subjectId || scopes.length !== r.value.scopes.length) {
      r.subjectId = actor.subjectId;
      r.value.scopes = scopes;
      await saveLive(r, signal);
      r.revision++;
    }
    const c = await client(r.value.clientId);
    return {
      r,
      actor,
      view: {
        requestId,
        client: { id: c.id, name: c.name },
        resource,
        scopes: [...r.value.scopes],
        expiresAt: r.expiresAt,
      } satisfies OAuthConsent,
    };
  }
  async function tokens(grantId: string, g: Grant) {
    const access = `loa_${grantId}.${random()}`,
      refresh = `lor_${grantId}.${random()}`;
    return {
      access,
      refresh,
      value: {
        ...g,
        accessHash: await oauthDigest(access),
        refreshHash: await oauthDigest(refresh),
        accessExpiresAt: Math.min(g.expiresAt, now() + 300_000),
      },
    };
  }
  function tokenResponse(value: Awaited<ReturnType<typeof tokens>>) {
    const remaining = value.value.accessExpiresAt - now();
    if (remaining <= 0) fail("invalid_grant");
    return {
      access_token: value.access,
      token_type: "Bearer",
      expires_in: Math.floor(remaining / 1000),
      refresh_token: value.refresh,
      scope: value.value.scopes.join(" "),
    };
  }
  async function verify(credential: string, signal: AbortSignal) {
    const match = tokenPattern.exec(credential);
    if (match?.[1] !== "loa") fail("invalid_grant");
    const r = await record<Grant>("grant", match[2], signal);
    const g = r.value;
    if (
      g.revoked ||
      g.expiresAt <= now() ||
      g.resource !== resource ||
      g.accessExpiresAt <= now() ||
      !equal(g.accessHash, await oauthDigest(credential))
    )
      fail("invalid_grant");
    const permissions = await allowed(g.subjectId, signal);
    if (g.expiresAt <= now() || g.accessExpiresAt <= now()) fail("invalid_grant");
    return {
      ...publicGrant(g),
      scopes: g.scopes.filter((s) => permissions.includes(s)),
      issuer,
      audience: [resource],
      accessExpiresAt: g.accessExpiresAt,
    };
  }
  function sourceForScopes(requiredScopes: readonly string[]) {
    // Capture operation requirements at assembly time, never from request JSON.
    const required = Object.freeze([...requiredScopes]);
    if (required.some((scope) => !options.scopes.includes(scope)))
      throw new AuthConfigurationError("OAuth source requires registered scopes");
    return Object.freeze(
      defineSource<string>({
        realmId: options.humanAccess.realmId,
        capabilities: Object.freeze({ authoritative: true }),
        async verify(credential, { signal }) {
          try {
            const g = await verify(credential, signal);
            if (!required.every((scope) => g.scopes.includes(scope)))
              throw new AuthError("FORBIDDEN");
            return {
              status: "verified",
              subjectId: g.subjectId,
              kind: "user",
              session: { expiresAt: g.accessExpiresAt, authoritative: true },
            };
          } catch (error) {
            if (error instanceof OAuthError) return { status: "rejected" };
            throw error;
          }
        },
      }),
    );
  }
  return {
    issuer,
    resource,
    metadata: {
      issuer,
      authorization_endpoint: issuer + "/authorize",
      token_endpoint: issuer + "/token",
      revocation_endpoint: issuer + "/revoke",
      response_types_supported: ["code"],
      grant_types_supported: ["authorization_code", "refresh_token"],
      token_endpoint_auth_methods_supported: ["none"],
      code_challenge_methods_supported: ["S256"],
      scopes_supported: [...options.scopes],
      authorization_response_iss_parameter_supported: true,
      ...(options.registration?.enabled ? { registration_endpoint: issuer + "/register" } : {}),
    },
    async register(input: unknown, signal: AbortSignal) {
      if (!options.registration?.enabled) fail("invalid_client");
      if (!input || typeof input !== "object" || Array.isArray(input)) fail("invalid_request");
      const p = input as Record<string, unknown>;
      if (p.token_endpoint_auth_method !== undefined && p.token_endpoint_auth_method !== "none")
        fail("invalid_client");
      const redirects = p.redirect_uris;
      if (
        !Array.isArray(redirects) ||
        !redirects.length ||
        redirects.length > (options.registration.maxRedirectUris ?? 4) ||
        redirects.some((r) => typeof r !== "string" || r.length > 1024)
      )
        fail("invalid_request");
      // DCR is public native-client registration. No outbound metadata fetch, custom
      // URI schemes, arbitrary remote redirects, or client-supplied privileges.
      for (const r of redirects) {
        let u: URL;
        try {
          u = trustedUrl(r);
        } catch {
          fail("invalid_request");
        }
        if (
          u.protocol !== "http:" ||
          !["127.0.0.1", "[::1]"].includes(u.hostname) ||
          u.search ||
          !u.pathname.startsWith("/callback")
        )
          fail("invalid_request");
      }
      if (
        p.grant_types !== undefined &&
        (!Array.isArray(p.grant_types) ||
          p.grant_types.some((v) => !["authorization_code", "refresh_token"].includes(v)))
      )
        fail("invalid_request");
      if (
        p.response_types !== undefined &&
        (!Array.isArray(p.response_types) ||
          p.response_types.length !== 1 ||
          p.response_types[0] !== "code")
      )
        fail("invalid_request");
      const name = p.client_name === undefined ? "Native MCP client" : p.client_name;
      if (
        typeof name !== "string" ||
        !name.trim() ||
        name.length > 120 ||
        Array.from(name).some(
          (character) => character.charCodeAt(0) < 32 || character.charCodeAt(0) === 127,
        )
      )
        fail("invalid_request");
      const c: OAuthPublicClient = {
        id: "loc_" + crypto.randomUUID(),
        name,
        redirectUris: [...new Set(redirects)],
      };
      signal.throwIfAborted();
      if (
        !(await options.store.insert({
          kind: "client",
          id: c.id,
          subjectId: "",
          revision: 0,
          expiresAt: now() + 365 * 86400_000,
          value: c,
        }))
      )
        fail("invalid_request");
      return {
        client_id: c.id,
        client_id_issued_at: Math.floor(now() / 1000),
        client_name: c.name,
        redirect_uris: c.redirectUris,
        grant_types: ["authorization_code", "refresh_token"],
        response_types: ["code"],
        token_endpoint_auth_method: "none",
      };
    },
    async begin(parameters: URLSearchParams, signal: AbortSignal) {
      const one = (name: string) => {
        const all = parameters.getAll(name);
        if (all.length > 1) fail("invalid_request");
        return all[0];
      };
      const c = await client(one("client_id") ?? "");
      const uri = one("redirect_uri") ?? "";
      if (
        one("response_type") !== "code" ||
        !c.redirectUris.some((r) => oauthRedirectMatches(r, uri)) ||
        one("code_challenge_method") !== "S256" ||
        !/^[-_A-Za-z0-9]{43}$/.test(one("code_challenge") ?? "")
      )
        fail("invalid_request");
      if (one("resource") !== resource) fail("invalid_target");
      const scopes = (one("scope") ?? "").split(" ").filter(Boolean);
      if (
        !scopes.length ||
        new Set(scopes).size !== scopes.length ||
        !scopes.every((s) => options.scopes.includes(s))
      )
        fail("invalid_scope");
      const state = one("state");
      if (state !== undefined && state.length > 1024) fail("invalid_request");
      const requestId = random();
      signal.throwIfAborted();
      if (
        !(await options.store.insert({
          kind: "request",
          id: await oauthDigest(requestId),
          subjectId: "",
          revision: 0,
          expiresAt: now() + 300_000,
          value: {
            clientId: c.id,
            redirect: uri,
            state,
            challenge: one("code_challenge")!,
            scopes,
            used: false,
          } satisfies Pending,
        }))
      )
        fail("invalid_request");
      return { requestId };
    },
    async consent(requestId: string, evidence: E, signal: AbortSignal) {
      return (await readConsent(requestId, evidence, signal)).view;
    },
    async decide(requestId: string, approve: boolean, evidence: E, signal: AbortSignal) {
      const { r, actor } = await readConsent(requestId, evidence, signal);
      r.value.used = true;
      await saveLive(r, signal);
      if (!approve) return { redirect: redirect(r.value, { error: "access_denied" }) };
      const c = await client(r.value.clientId),
        id = crypto.randomUUID(),
        expiresAt = now() + 7 * 86400_000;
      const g: Grant = {
        id,
        clientId: c.id,
        clientName: c.name,
        subjectId: actor.subjectId,
        resource,
        scopes: r.value.scopes,
        createdAt: now(),
        expiresAt,
        revoked: false,
        accessHash: "",
        accessExpiresAt: 0,
        refreshHash: "",
        usedRefresh: [],
      };
      if (
        !(await options.store.insert({
          kind: "grant",
          id,
          subjectId: actor.subjectId,
          expiresAt,
          revision: 0,
          value: g,
        }))
      )
        fail("invalid_grant");
      const code = random();
      signal.throwIfAborted();
      if (
        !(await options.store.insert({
          kind: "code",
          id: await oauthDigest(code),
          subjectId: actor.subjectId,
          expiresAt: now() + 60_000,
          revision: 0,
          value: {
            clientId: c.id,
            redirect: r.value.redirect,
            challenge: r.value.challenge,
            grantId: id,
            used: false,
          } satisfies Code,
        }))
      )
        fail("invalid_grant");
      return { redirect: redirect(r.value, { code }) };
    },
    async exchange(p: URLSearchParams, signal: AbortSignal) {
      for (const key of p.keys()) if (p.getAll(key).length !== 1) fail("invalid_request");
      const c = await client(p.get("client_id") ?? "");
      if (p.has("client_secret")) fail("invalid_client");
      if (p.get("resource") !== resource) fail("invalid_target");
      if (p.get("grant_type") === "authorization_code") {
        const r = await record<Code>("code", await oauthDigest(p.get("code") ?? ""), signal),
          v = p.get("code_verifier") ?? "";
        if (
          r.value.used ||
          r.value.clientId !== c.id ||
          r.value.redirect !== p.get("redirect_uri") ||
          !/^[-._~A-Za-z0-9]{43,128}$/.test(v) ||
          !equal(await challenge(v), r.value.challenge)
        )
          fail("invalid_grant");
        const grant = await record<Grant>("grant", r.value.grantId, signal);
        if (grant.value.revoked) fail("invalid_grant");
        const permissions = await allowed(grant.subjectId, signal);
        if (!grant.value.scopes.every((s) => permissions.includes(s))) fail("invalid_scope");
        r.value.used = true;
        await saveLive(r, signal);
        const next = await tokens(grant.id, grant.value);
        grant.value = next.value;
        await saveLive(grant, signal);
        return tokenResponse(next);
      }
      if (p.get("grant_type") !== "refresh_token") fail("unsupported_grant_type");
      const credential = p.get("refresh_token") ?? "",
        match = tokenPattern.exec(credential);
      if (match?.[1] !== "lor") fail("invalid_grant");
      const r = await record<Grant>("grant", match[2], signal),
        hash = await oauthDigest(credential),
        g = r.value;
      if (g.clientId !== c.id || g.revoked) fail("invalid_grant");
      if (!equal(hash, g.refreshHash)) {
        if (g.usedRefresh.includes(hash)) {
          g.revoked = true;
          await save(r, signal);
        }
        fail("invalid_grant");
      }
      const requested = (p.get("scope") ?? g.scopes.join(" ")).split(" ").filter(Boolean),
        permissions = await allowed(g.subjectId, signal);
      if (
        !requested.length ||
        !requested.every((s) => g.scopes.includes(s) && permissions.includes(s))
      )
        fail("invalid_scope");
      if (g.usedRefresh.length >= 2048) {
        g.revoked = true;
        await save(r, signal);
        fail("invalid_grant");
      }
      const next = await tokens(r.id, {
        ...g,
        scopes: requested,
        usedRefresh: [...g.usedRefresh, hash],
      });
      r.value = next.value;
      await saveLive(r, signal);
      return tokenResponse(next);
    },
    verify,
    /** Identity only. Select sourceForScopes for an operation's delegated authority. */
    source: sourceForScopes([]),
    /** Fresh grant and current scope checks in the same Auth verification. */
    sourceForScopes(requiredScopes: readonly string[]) {
      if (!requiredScopes.length)
        throw new AuthConfigurationError("At least one OAuth scope is required");
      return sourceForScopes(requiredScopes);
    },
    async grants(evidence: E, signal: AbortSignal) {
      const actor = await human(evidence, signal);
      await allowed(actor.subjectId, signal);
      return (await options.store.list("grant", actor.subjectId)).map((r) =>
        publicGrant(r.value as Grant),
      );
    },
    async revokeGrant(id: string, evidence: E, signal: AbortSignal) {
      const actor = await human(evidence, signal),
        r = await record<Grant>("grant", id, signal);
      if (r.subjectId !== actor.subjectId) fail("access_denied");
      r.value.revoked = true;
      await save(r, signal);
    },
    async revokeToken(clientId: string, credential: string, signal: AbortSignal) {
      signal.throwIfAborted();
      await client(clientId);
      signal.throwIfAborted();
      const m = tokenPattern.exec(credential);
      if (!m) return;
      const raw = await options.store.read("grant", m[2]);
      signal.throwIfAborted();
      if (!raw) return;
      const hash = await oauthDigest(credential);
      signal.throwIfAborted();
      const inactive = (entry: OAuthRecord | null) => {
        if (!entry) return true;
        const g = entry.value as Grant;
        return (
          g.clientId !== clientId ||
          g.revoked ||
          entry.expiresAt <= now() ||
          g.expiresAt <= now() ||
          !equal(hash, m[1] === "loa" ? g.accessHash : g.refreshHash)
        );
      };
      // RFC7009 success must not distinguish missing, expired or revoked tokens.
      if (inactive(raw)) return;
      const r = { ...raw, value: { ...(raw.value as Grant), revoked: true } };
      let changed = false;
      let failure: unknown;
      try {
        changed = (await options.store.replace(r, r.revision)) === true;
      } catch (error) {
        failure = error;
      }
      signal.throwIfAborted();
      if (changed) return;
      // A competing write or lost acknowledgement needs one authoritative read,
      // never another CAS. Do not claim success while the token is still live.
      const current = await options.store.read("grant", m[2]);
      signal.throwIfAborted();
      if (!inactive(current)) throw new AuthError("SERVICE_UNAVAILABLE", { cause: failure });
    },
    async close() {
      /* Store is borrowed; no background resources. */
    },
  };
}
export type OAuthAuthorizationServer<E = unknown> = ReturnType<
  typeof createOAuthAuthorizationServer<E>
>;
