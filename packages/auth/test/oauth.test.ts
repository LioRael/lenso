import { test, expect } from "bun:test";
import { Database } from "bun:sqlite";
import { audience, createAuth, defineSource, realm, AuthError } from "@lenso/auth";
import { authErrorResponse, bearerEvidence } from "@lenso/auth/fetch";
import { createOAuthAuthorizationServer, createSqlOAuthStore } from "@lenso/auth/oauth";
import { createOAuthHttpBinding } from "@lenso/auth/oauth-http";

// Prevent cross-resource redemption, replay and privilege revival on the actual
// SQL CAS boundary. Existing session tests never exercise OAuth records.
async function fixture() {
  const db = new Database(":memory:");
  const io = { grantReads: 0, scopeReads: 0 };
  db.exec(await Bun.file(new URL("../migrations/oauth/0000_oauth.sql", import.meta.url)).text());
  const store = createSqlOAuthStore(
    {
      async query(sql, values) {
        if (sql.startsWith("SELECT") && values[1] === "grant") io.grantReads++;
        return db.query(sql).all(...(values as any[])) as Record<string, unknown>[];
      },
    },
    "test",
    "sqlite",
  );
  const state = {
    now: Date.now(),
    enabled: true,
    scopes: ["users.list", "users.read"],
    scopeReadDelay: 0,
  };
  const auth = createAuth(
    realm(
      "test",
      defineSource<string>({
        async verify(e) {
          return e === "human"
            ? { status: "verified", subjectId: "alice", kind: "user" }
            : e === "bob"
              ? { status: "verified", subjectId: "bob", kind: "user" }
              : { status: "rejected" };
        },
      }),
    ),
  );
  const server = createOAuthAuthorizationServer({
    issuer: "http://127.0.0.1:31415/oauth",
    resource: "http://127.0.0.1:31415/api/mcp",
    clients: [],
    registration: { enabled: true },
    scopes: state.scopes,
    store,
    humanAccess: auth.for(audience("consent")),
    async allowedScopes() {
      io.scopeReads++;
      state.now += state.scopeReadDelay;
      return state.enabled ? state.scopes : null;
    },
    now: () => state.now,
  });
  const signal = new AbortController().signal,
    verifier = "a".repeat(43);
  const bytes = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(verifier));
  const challenge = Buffer.from(bytes).toString("base64url");
  const client = await server.register(
    {
      client_name: "Codex",
      redirect_uris: ["http://127.0.0.1/callback/native"],
      token_endpoint_auth_method: "none",
    },
    signal,
  );
  const authorize = () =>
    new URLSearchParams({
      client_id: client.client_id,
      response_type: "code",
      redirect_uri: "http://127.0.0.1:19001/callback/native",
      resource: server.resource,
      code_challenge: challenge,
      code_challenge_method: "S256",
      scope: "users.list users.read",
      state: "roundtrip",
    });
  async function issue() {
    const pending = await server.begin(authorize(), signal);
    await server.consent(pending.requestId, "human", signal);
    const approved = new URL(
      (await server.decide(pending.requestId, true, "human", signal)).redirect,
    );
    const exchange = new URLSearchParams({
      grant_type: "authorization_code",
      client_id: client.client_id,
      code: approved.searchParams.get("code")!,
      code_verifier: verifier,
      redirect_uri: authorize().get("redirect_uri")!,
      resource: server.resource,
    });
    const tokens = await server.exchange(exchange, signal);
    return { pending, approved, exchange, tokens };
  }
  return {
    server,
    state,
    signal,
    client,
    authorize,
    issue,
    store,
    db,
    io,
    async close() {
      await auth.close();
      db.close();
    },
  };
}

test("DCR, consent, PKCE and resource binding; code and decision are one-shot", async () => {
  const f = await fixture();
  try {
    await expect(
      f.server.register({ redirect_uris: ["https://attacker.invalid/callback"] }, f.signal),
    ).rejects.toThrow("invalid_request");
    const p = f.authorize();
    p.set("resource", "https://other.invalid/mcp");
    await expect(f.server.begin(p, f.signal)).rejects.toThrow("invalid_target");
    const pending = await f.server.begin(f.authorize(), f.signal);
    await f.server.consent(pending.requestId, "human", f.signal);
    await expect(f.server.decide(pending.requestId, true, "bob", f.signal)).rejects.toThrow(
      "invalid_grant",
    );
    const denied = new URL(
      (await f.server.decide(pending.requestId, false, "human", f.signal)).redirect,
    );
    expect(denied.searchParams.get("error")).toBe("access_denied");
    expect(denied.searchParams.get("state")).toBe("roundtrip");
    await expect(f.server.decide(pending.requestId, true, "human", f.signal)).rejects.toThrow(
      "invalid_grant",
    );
    f.state.scopes = ["users.list"];
    const x = await f.issue();
    expect(x.approved.searchParams.get("iss")).toBe(f.server.issuer);
    const g = await f.server.verify(x.tokens.access_token, f.signal);
    expect(g.subjectId).toBe("alice");
    expect(g.scopes).toEqual(["users.list"]);
    expect(g.audience).toEqual([f.server.resource]);
    await expect(f.server.exchange(x.exchange, f.signal)).rejects.toThrow("invalid_grant");
    expect(JSON.stringify(await f.server.grants("human", f.signal))).not.toContain(
      x.tokens.access_token,
    );
    const raw = await f.store.read("grant", g.id);
    expect(JSON.stringify(raw)).not.toContain(x.tokens.refresh_token);
  } finally {
    await f.close();
  }
});

test("public OAuth source composes one verification with current object membership in a Fetch app", async () => {
  const f = await fixture();
  const requiredScopes = ["users.read"];
  const source = f.server.sourceForScopes(requiredScopes);
  requiredScopes.length = 0; // Assembly captures requirements, not the caller's mutable array.
  expect(() => realm("another-realm", source)).toThrow("different realm");
  expect(() => realm("another-realm", f.server.source)).toThrow("different realm");
  expect(() => f.server.sourceForScopes([])).toThrow();
  expect(() => f.server.sourceForScopes(["unregistered"])).toThrow();
  const auth = createAuth(realm("test", source), { now: () => f.state.now });
  const identityAuth = createAuth(realm("test", f.server.source), { now: () => f.state.now });
  f.db.exec("CREATE TABLE membership (subject TEXT, tenant TEXT, active INTEGER)");
  f.db.exec("INSERT INTO membership VALUES ('alice', 'north', 1)");
  let membershipReads = 0;
  const access = auth.for(audience("user-details")).memberships(async (subject, tenant: string) => {
    membershipReads++;
    return f.db
      .query("SELECT active FROM membership WHERE subject = ? AND tenant = ?")
      .get(subject.subjectId, tenant) as { active: number } | null;
  });
  const policy = ({ membership }: { membership: { active: number } }) => membership.active === 1;
  // A complete listener-independent Fetch entry; the same service policy can be used by MCP.
  const app = {
    async fetch(request: Request) {
      try {
        const { evidence: token } = bearerEvidence({ request });
        if (!token) throw new AuthError("UNAUTHORIZED");
        const actor = await access.authorized(
          token,
          new URL(request.url).pathname.slice(1),
          policy,
          {
            signal: request.signal,
          },
        );
        return Response.json({ subject: actor.subjectId });
      } catch (error) {
        return authErrorResponse(error);
      }
    },
  };
  try {
    const { tokens } = await f.issue();
    const call = (tenant = "north") =>
      app.fetch(
        new Request(`https://app.test/${tenant}`, {
          headers: { authorization: `Bearer ${tokens.access_token}` },
        }),
      );
    f.io.grantReads = f.io.scopeReads = 0;
    // Relay's old composition verifies once for scopes and again for the Auth actor.
    await f.server.verify(tokens.access_token, f.signal);
    await identityAuth.for(audience("user-details")).required(tokens.access_token);
    expect(f.io).toEqual({ grantReads: 2, scopeReads: 2 });
    f.io.grantReads = f.io.scopeReads = 0;
    expect(await (await call()).json()).toEqual({ subject: "alice" });
    expect(f.io).toEqual({ grantReads: 1, scopeReads: 1 });
    expect(membershipReads).toBe(1);
    expect((await call("south")).status).toBe(403);
    const actor = await access.authorized(tokens.access_token, "north", policy);
    f.db.exec("UPDATE membership SET active = 0");
    await expect(access.enforce(actor, "north", policy)).rejects.toMatchObject({
      code: "FORBIDDEN",
    });
    f.db.exec("UPDATE membership SET active = 1");
    f.state.scopes = ["users.list"];
    expect((await call()).status).toBe(403);
    await expect(access.enforce(actor, "north", policy)).rejects.toMatchObject({
      code: "FORBIDDEN",
    });
    f.state.scopes = ["users.list", "users.read"];
    expect((await call()).status).toBe(200);
    const readActor = await access.required(tokens.access_token, { signal: f.signal });
    const read = await access.beginRead!(readActor, { signal: f.signal });
    try {
      await access.enforceRead!(read, "north", policy, { signal: f.signal });
      f.state.scopes = ["users.list"];
      await expect(read.finish()).rejects.toMatchObject({ code: "FORBIDDEN" });
    } finally {
      read.close();
    }
    const limited = await f.issue();
    f.state.scopes = ["users.list", "users.read"];
    // Restoring account permissions cannot add scopes that were never consented.
    await expect(
      access.authorized(limited.tokens.access_token, "north", policy),
    ).rejects.toMatchObject({ code: "FORBIDDEN" });
    await f.server.revokeToken(f.client.client_id, tokens.access_token, f.signal);
    expect((await call()).status).toBe(401);
    await expect(access.enforce(actor, "north", policy)).rejects.toMatchObject({
      code: "UNAUTHORIZED",
    });
  } finally {
    await auth.close();
    await identityAuth.close();
    await f.close();
  }
});

test("direct OAuth verification cannot return a token that expired while reading permissions", async () => {
  const f = await fixture();
  try {
    const { tokens } = await f.issue();
    f.state.scopeReadDelay = tokens.expires_in * 1000;
    await expect(f.server.verify(tokens.access_token, f.signal)).rejects.toThrow("invalid_grant");
  } finally {
    await f.close();
  }
});

test("refresh rotation/replay revokes family, current membership and expiry are authoritative", async () => {
  const f = await fixture();
  try {
    const x = await f.issue();
    const p = new URLSearchParams({
      grant_type: "refresh_token",
      client_id: f.client.client_id,
      resource: f.server.resource,
      refresh_token: x.tokens.refresh_token,
    });
    const next = await f.server.exchange(p, f.signal);
    await expect(f.server.verify(x.tokens.access_token, f.signal)).rejects.toThrow("invalid_grant");
    expect((await f.server.verify(next.access_token, f.signal)).subjectId).toBe("alice");
    await expect(f.server.exchange(p, f.signal)).rejects.toThrow("invalid_grant");
    await expect(f.server.verify(next.access_token, f.signal)).rejects.toThrow("invalid_grant");
    const second = await f.issue();
    f.state.scopes = ["users.list"];
    expect((await f.server.verify(second.tokens.access_token, f.signal)).scopes).toEqual([
      "users.list",
    ]);
    f.state.enabled = false;
    await expect(f.server.verify(second.tokens.access_token, f.signal)).rejects.toThrow(
      "access_denied",
    );
    f.state.enabled = true;
    f.state.now += 300_001;
    await expect(f.server.verify(second.tokens.access_token, f.signal)).rejects.toThrow(
      "invalid_grant",
    );
  } finally {
    await f.close();
  }
});

test("HTTP uses same-origin authenticated consent, rejects cookie/token confusion and duplicate inputs", async () => {
  const f = await fixture();
  try {
    const http = createOAuthHttpBinding({
      server: f.server,
      consentPage: "/console/oauth",
      evidence: (r) => r.headers.get("x-test-human") ?? "",
      admitHuman(r) {
        if (r.method === "POST" && r.headers.get("x-test-csrf") !== "valid")
          throw new Error("denied");
      },
      admitPublic() {},
    });
    const call = (path: string, init?: RequestInit) =>
      http.fetch(new Request("http://127.0.0.1:31415" + path, init));
    expect((await call("/.well-known/oauth-authorization-server/oauth"))?.status).toBe(200);
    const unavailable = createOAuthHttpBinding({
      server: f.server,
      consentPage: "/console/oauth",
      evidence: () => "human",
      admitHuman() {
        throw new AuthError("SERVICE_UNAVAILABLE");
      },
      admitPublic() {},
    });
    expect(
      (await unavailable.fetch(new Request("http://127.0.0.1:31415/oauth/grants")))?.status,
    ).toBe(503);
    const location = (await call("/oauth/authorize?" + f.authorize()))!.headers.get("location")!;
    const id = new URL(location).searchParams.get("request_id")!;
    expect((await call("/oauth/consent?request_id=" + id))?.status).toBe(401);
    expect(
      (
        await call("/oauth/decision", {
          method: "POST",
          headers: {
            origin: "https://other.invalid",
            "x-test-human": "human",
            "content-type": "application/json",
          },
          body: JSON.stringify({ requestId: id, approve: true }),
        })
      )?.status,
    ).toBe(403);
    expect(
      (await call("/oauth/consent?request_id=" + id, { headers: { "x-test-human": "human" } }))
        ?.status,
    ).toBe(200);
    expect(
      (
        await call("/oauth/token", {
          method: "POST",
          headers: {
            cookie: "session=anything",
            "content-type": "application/x-www-form-urlencoded",
          },
          body: "grant_type=authorization_code",
        })
      )?.status,
    ).toBe(400);
    const x = await f.issue();
    x.exchange.append("client_id", f.client.client_id);
    await expect(f.server.exchange(x.exchange, f.signal)).rejects.toThrow("invalid_request");
    await f.server.revokeToken(f.client.client_id, x.tokens.refresh_token, f.signal);
    await expect(f.server.verify(x.tokens.access_token, f.signal)).rejects.toThrow("invalid_grant");
  } finally {
    await f.close();
  }
});

test("OAuth HTTP separates malformed request bodies from admission and storage failures", async () => {
  const f = await fixture();
  let publicError: Error | undefined;
  const http = createOAuthHttpBinding({
    server: f.server,
    consentPage: "/console/oauth",
    evidence: () => "human",
    admitHuman() {},
    admitPublic() {
      if (publicError) throw publicError;
    },
  });
  const post = (path: string, body: BodyInit, type = "application/json") =>
    http.fetch(
      new Request("http://127.0.0.1:31415/oauth/" + path, {
        method: "POST",
        headers: { origin: "http://127.0.0.1:31415", "content-type": type },
        body,
      }),
    );
  try {
    for (const path of ["register", "decision", "grants/revoke"]) {
      for (const body of ["{", new Uint8Array([0xff]), new Uint8Array([0xe2, 0x82])]) {
        const response = (await post(path, body))!;
        expect(response.status).toBe(400);
        expect(await response.json()).toEqual({ error: "invalid_request" });
        expect(response.headers.get("cache-control")).toBe("no-store");
      }
    }
    expect(
      (await post("token", new Uint8Array([0xff]), "application/x-www-form-urlencoded"))?.status,
    ).toBe(400);
    // Only parser failures in the client body are 400. Same error class elsewhere is still 503.
    publicError = new SyntaxError("private backend detail");
    const failed = (await post("register", "{}"))!;
    expect(failed.status).toBe(503);
    expect(await failed.json()).toEqual({ error: "server_error" });
    publicError = undefined;
    await f.issue();
    f.db.exec("UPDATE auth_oauth_records SET document = '{' WHERE kind = 'grant'");
    const storage = (await http.fetch(new Request("http://127.0.0.1:31415/oauth/grants")))!;
    expect(storage.status).toBe(503);
    expect(await storage.json()).toEqual({ error: "server_error" });
  } finally {
    await f.close();
  }
});

// Bad PKCE/resource inputs must not consume a legitimate code, and parallel
// refresh redemption may create only one successor across independent requests.
test("wrong verifier/resource cannot redeem; SQL CAS permits one concurrent refresh", async () => {
  const f = await fixture();
  try {
    const pending = await f.server.begin(f.authorize(), f.signal);
    const callback = new URL(
      (await f.server.decide(pending.requestId, true, "human", f.signal)).redirect,
    );
    const p = new URLSearchParams({
      grant_type: "authorization_code",
      client_id: f.client.client_id,
      code: callback.searchParams.get("code")!,
      code_verifier: "wrong".repeat(10),
      redirect_uri: f.authorize().get("redirect_uri")!,
      resource: f.server.resource,
    });
    await expect(f.server.exchange(p, f.signal)).rejects.toThrow("invalid_grant");
    p.set("code_verifier", "a".repeat(43));
    p.set("resource", "https://other.invalid/mcp");
    await expect(f.server.exchange(p, f.signal)).rejects.toThrow("invalid_target");
    p.set("resource", f.server.resource);
    const token = await f.server.exchange(p, f.signal);
    const refresh = new URLSearchParams({
      grant_type: "refresh_token",
      client_id: f.client.client_id,
      resource: f.server.resource,
      refresh_token: token.refresh_token,
    });
    const results = await Promise.allSettled([
      f.server.exchange(refresh, f.signal),
      f.server.exchange(refresh, f.signal),
    ]);
    expect(results.filter((v) => v.status === "fulfilled")).toHaveLength(1);
    const successor = results.find((v) => v.status === "fulfilled") as PromiseFulfilledResult<
      typeof token
    >;
    const grant = await f.server.verify(successor.value.access_token, f.signal);
    await expect(f.server.revokeGrant(grant.id, "bob", f.signal)).rejects.toThrow("access_denied");
    await f.server.revokeGrant(grant.id, "human", f.signal);
    await expect(f.server.verify(successor.value.access_token, f.signal)).rejects.toThrow(
      "invalid_grant",
    );
    expect(await f.store.pruneExpired!(f.state.now + 366 * 86400_000, 32)).toBeGreaterThan(0);
  } finally {
    await f.close();
  }
});

// Entry authentication alone cannot protect an asynchronous DTO disclosure.
test("HTTP withholds private results when the Console identity retires during IO", async () => {
  const f = await fixture();
  try {
    await f.issue();
    let admissions = 0;
    const http = createOAuthHttpBinding({
      server: f.server,
      consentPage: "/console/oauth",
      evidence: () => "human",
      admitPublic() {},
      admitHuman() {
        if (++admissions > 1) throw new AuthError("FORBIDDEN");
      },
    });
    const response = await http.fetch(new Request("http://127.0.0.1:31415/oauth/grants"));
    expect(response?.status).toBe(403);
    expect(await response?.text()).not.toContain("Codex");
    expect(admissions).toBe(2);
  } finally {
    await f.close();
  }
});
