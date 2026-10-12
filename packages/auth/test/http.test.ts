import { Database } from "bun:sqlite";
import { expect, test } from "bun:test";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { drizzle } from "drizzle-orm/bun-sqlite";
import { audience, createAuth, realm } from "../src/core";
import { sqliteSessionStore } from "../src/drizzle/sqlite";
import {
  assertAuthHttpBindings,
  authCookie,
  createAuthHttpBinding,
  safeAuthReturnPath,
} from "../src/http";
import { externalEntry, passwordLogin, type PasswordInput } from "../src/interactive";
import { createManagedSessions } from "../src/sessions";

const origin = "https://console.test";
const sessionName = "__Host-console-session";
const csrfName = "__Host-console-csrf";
const policy = {
  origin,
  credentialMode: "cookie" as const,
  sessionCookieName: sessionName,
  csrfCookieName: csrfName,
} as const;
const request = (
  path: string,
  credential?: string,
  method = "GET",
  input?: unknown,
  extra?: HeadersInit,
) =>
  new Request(`${origin}${path}`, {
    method,
    headers: {
      origin,
      cookie: `${csrfName}=csrf${credential ? `; ${sessionName}=${credential}` : ""}`,
      "x-csrf-token": "csrf",
      "content-type": "application/json",
      ...extra,
    },
    ...(input === undefined ? {} : { body: JSON.stringify(input) }),
  });
function sessionCredential(response: Response) {
  expect(response.headers.get("set-cookie")).toContain("HttpOnly");
  return /__Host-console-session=([^;]+)/u.exec(response.headers.get("set-cookie")!)![1]!;
}

// Existing session-store tests cannot prove HTTP issuance/restart/CSRF boundaries.
test("password HTTP uses persistent SQLite sessions, CAS rotation and isolated logout", async () => {
  const directory = await mkdtemp(join(tmpdir(), "auth-http-"));
  let client = new Database(join(directory, "auth.sqlite"));
  let time = Date.now();
  const services: { close(): Promise<void> }[] = [];
  try {
    client.exec(
      await readFile(
        new URL("../migrations/sqlite/0000_auth_sessions.sql", import.meta.url),
        "utf8",
      ),
    );
    function assemble(realmId = "console", kind: "user" | "service" = "user") {
      const sessions = createManagedSessions({
        realmId,
        store: sqliteSessionStore(drizzle(client)),
        now: () => time,
        lifetime: { idle: 60_000, absolute: 120_000, renewAfter: 20 },
        subjectActive: async () => true,
        login: {
          async verify(input: PasswordInput) {
            return input.identifier === "alice" && input.password === "test-password"
              ? { status: "verified" as const, subjectId: "alice", kind }
              : { status: "rejected" as const };
          },
        },
      });
      const auth = createAuth(realm(realmId, sessions.source), { now: () => time });
      services.push(auth, sessions);
      const access = auth.for(audience("console"));
      const binding = createAuthHttpBinding({
        access,
        requestPolicy: policy,
        evidence: (r) => authCookie(r, sessionName),
        methodsPath: "/auth/methods",
        returnRoots: ["/console"],
        sessions,
        logoutPath: "/auth/logout",
        renewPath: "/auth/renew",
        login: [
          passwordLogin({
            id: "password",
            label: "Account",
            path: "/auth/login",
            sessions,
          }),
        ],
      });
      return { sessions, auth, access, binding };
    }
    let consoleAuth = assemble();
    const discovery = await consoleAuth.binding.fetch(new Request(`${origin}/auth/methods`));
    expect(discovery!.headers.get("set-cookie")).toContain(`${csrfName}=`);
    expect(await discovery!.json()).toEqual({
      methods: [{ id: "password", kind: "password", label: "Account", action: "/auth/login" }],
      session: { logout: "/auth/logout", renew: "/auth/renew" },
      csrf: { cookie_name: csrfName, header_name: "x-csrf-token" },
    });
    const input = { identifier: "alice", password: "test-password" };
    expect(
      (await consoleAuth.binding.fetch(
        request("/auth/login", undefined, "POST", input, { origin: "https://attacker.test" }),
      ))!.status,
    ).toBe(403);
    expect(
      (await consoleAuth.binding.fetch(
        request("/auth/login", undefined, "POST", input, { "x-csrf-token": "wrong" }),
      ))!.status,
    ).toBe(403);
    expect(
      (await consoleAuth.binding.fetch(
        request("/auth/login", undefined, "POST", {
          ...input,
          password: "wrong",
        }),
      ))!.status,
    ).toBe(401);
    expect(
      (await consoleAuth.binding.fetch(
        request("/auth/login", undefined, "POST", {
          ...input,
          password: "x".repeat(8192),
        }),
      ))!.status,
    ).toBe(413);
    const machine = assemble("machine", "service");
    expect(
      (await machine.binding.fetch(request("/auth/login", undefined, "POST", input)))!.status,
    ).toBe(401);
    const login = (await consoleAuth.binding.fetch(
      request("/auth/login", undefined, "POST", input),
    ))!;
    expect(login.status).toBe(204);
    const original = sessionCredential(login);
    const business = assemble("business");
    const businessSession = await business.sessions.issue(input);
    await consoleAuth.auth.close();
    await consoleAuth.sessions.close();
    client.close();
    client = new Database(join(directory, "auth.sqlite"));
    consoleAuth = assemble();
    expect((await consoleAuth.access.required(original)).subjectId).toBe("alice");
    const earlyRenewal = (await consoleAuth.binding.fetch(
      request("/auth/renew", original, "POST"),
    ))!;
    expect(earlyRenewal.status).toBe(409);
    expect(await earlyRenewal.json()).toMatchObject({ code: "RENEWAL_NOT_DUE" });
    expect(earlyRenewal.headers.has("set-cookie")).toBe(false);
    expect((await consoleAuth.access.required(original)).subjectId).toBe("alice");
    expect(
      (await consoleAuth.binding.fetch(
        request("/auth/logout", original, "POST", undefined, { "x-lenso-expected-subject": "bob" }),
      ))!.status,
    ).toBe(412);
    time += 21;
    const renewals = await Promise.all([
      consoleAuth.binding.fetch(request("/auth/renew", original, "POST")),
      consoleAuth.binding.fetch(request("/auth/renew", original, "POST")),
    ]);
    expect(renewals.map((response) => response!.status).sort()).toEqual([204, 401]);
    const rotated = sessionCredential(renewals.find((response) => response!.status === 204)!);
    await expect(consoleAuth.access.required(original)).rejects.toMatchObject({
      code: "UNAUTHORIZED",
    });
    const logout = await consoleAuth.binding.fetch(request("/auth/logout", rotated, "POST"));
    expect(logout!.status).toBe(204);
    expect(logout!.headers.get("set-cookie")).toContain("Max-Age=0");
    await expect(consoleAuth.access.required(rotated)).rejects.toMatchObject({
      code: "UNAUTHORIZED",
    });
    expect((await consoleAuth.binding.fetch(request("/auth/renew", rotated, "POST")))!.status).toBe(
      401,
    );
    // Reopen the business owner against the same durable store after connection restart.
    const reopenedBusiness = assemble("business");
    expect((await reopenedBusiness.access.required(businessSession.credential)).subjectId).toBe(
      "alice",
    );
    const expiring = await consoleAuth.sessions.issue(input);
    time += 60_000;
    expect(
      (await consoleAuth.binding.fetch(request("/auth/renew", expiring.credential, "POST")))!
        .status,
    ).toBe(401);
    const otherAuth = createAuth(realm("console", consoleAuth.sessions.source));
    services.push(otherAuth);
    expect(() => consoleAuth.binding.assertAccess(otherAuth.for(audience("console")))).toThrow();
    expect(() =>
      consoleAuth.binding.assertAccess(consoleAuth.auth.for(audience("business"))),
    ).toThrow();
    consoleAuth.binding.assertAccess(consoleAuth.access.memberships(async () => ({})));
    expect(() => assertAuthHttpBindings([consoleAuth.binding, consoleAuth.binding])).toThrow();
    expect(() =>
      assertAuthHttpBindings([consoleAuth.binding], [{ path: "/auth/login", method: "POST" }]),
    ).toThrow();
  } finally {
    for (const service of services) await service.close();
    client.close();
    await rm(directory, { recursive: true, force: true });
  }
});

// Realm/runtime checks cannot distinguish independent stores in the same realm.
test("HTTP rejects a different managed session source even in the same realm", async () => {
  const clients = [new Database(":memory:"), new Database(":memory:")];
  const sessions = [];
  for (const client of clients) {
    client.exec(
      await readFile(
        new URL("../migrations/sqlite/0000_auth_sessions.sql", import.meta.url),
        "utf8",
      ),
    );
    sessions.push(
      createManagedSessions({
        realmId: "console",
        store: sqliteSessionStore(drizzle(client)),
        lifetime: { idle: 60_000, absolute: 120_000, renewAfter: 20 },
        subjectActive: async () => true,
        login: {
          async verify(_input: PasswordInput) {
            return { status: "verified" as const, subjectId: "alice" };
          },
        },
      }),
    );
  }
  const [a, b] = sessions;
  const auth = createAuth(realm("console", a!.source));
  try {
    const access = auth.for(audience("console")).requireSession({ authoritative: true });
    const base = {
      access,
      requestPolicy: policy,
      evidence: (r: Request) => authCookie(r, sessionName),
      methodsPath: "/auth/methods",
      returnRoots: ["/console"],
      logoutPath: "/auth/logout",
      renewPath: "/auth/renew",
    };
    const loginFor = (owner: typeof a) =>
      passwordLogin({ id: "password", label: "Account", path: "/auth/login", sessions: owner! });
    expect(() => createAuthHttpBinding({ ...base, sessions: b!, login: [loginFor(b)] })).toThrow(
      "same session source",
    );
    expect(() => createAuthHttpBinding({ ...base, sessions: a!, login: [loginFor(b)] })).toThrow(
      "same session owner",
    );
    const binding = createAuthHttpBinding({
      ...base,
      sessions: a!,
      login: [loginFor(a)],
    });
    const login = (await binding.fetch(
      request("/auth/login", undefined, "POST", {
        identifier: "alice",
        password: "test-password",
      }),
    ))!;
    expect(login.status).toBe(204);
    expect((await access.required(sessionCredential(login))).subjectId).toBe("alice");
  } finally {
    await auth.close();
    for (const owner of sessions) await owner.close();
    for (const client of clients) client.close();
  }
});

// A verification source alone must not imply login, and external adapter-private
// fields must not escape through browser discovery.
test("external entry is explicit, projects safe fields and binds a host callback", async () => {
  let providerSession = false;
  const auth = createAuth(
    realm("external", {
      verify: async () =>
        providerSession ? { status: "verified", subjectId: "alice" } : { status: "absent" },
    }),
  );
  try {
    const access = auth.for(audience("console"));
    const base = {
      access,
      requestPolicy: { origin, credentialMode: "bearer" as const },
      evidence: () => null,
      methodsPath: "/auth/methods",
      returnRoots: ["/console"],
    };
    const absent = createAuthHttpBinding(base);
    expect(await (await absent.fetch(request("/auth/methods")))!.json()).toEqual({ methods: [] });
    const descriptor = {
      id: "external",
      kind: "redirect" as const,
      label: "Company",
      action: "/auth/company",
      privateKey: "never-disclose",
    };
    const binding = createAuthHttpBinding({
      ...base,
      externalSession: {
        logout: {
          path: "/auth/logout",
          async handle() {
            providerSession = false;
            return new Response(null, {
              status: 204,
              headers: {
                "set-cookie": "__Host-external=; Path=/; Secure; HttpOnly; Max-Age=0",
              },
            });
          },
        },
      },
      login: [
        externalEntry({
          descriptor,
          async start(_request, context) {
            expect(context.returnPath).toBe("/console");
            return Response.redirect("https://identity.test/host-owned", 302);
          },
          callback: {
            path: "/auth/callback",
            method: "GET",
            async complete() {
              providerSession = true;
              return new Response(null, {
                status: 303,
                headers: {
                  location: "/console",
                  "set-cookie": "__Host-external=host-owned; Path=/; Secure; HttpOnly",
                },
              });
            },
          },
        }),
      ],
    });
    expect(
      JSON.stringify(await (await binding.fetch(request("/auth/methods")))!.json()),
    ).not.toContain("privateKey");
    expect(
      (await binding.fetch(request("/auth/company?return_to=https://attacker.test")))!.status,
    ).toBe(302);
    expect((await binding.fetch(request("/auth/callback")))!.headers.get("set-cookie")).toContain(
      "__Host-external=host-owned",
    );
    expect(
      (await binding.fetch(
        request("/auth/logout", undefined, "POST", undefined, {
          "x-lenso-expected-subject": "bob",
        }),
      ))!.status,
    ).toBe(412);
    expect(
      (await binding.fetch(request("/auth/logout", undefined, "POST")))!.headers.get("set-cookie"),
    ).toContain("Max-Age=0");
    expect(providerSession).toBe(false);
    expect(safeAuthReturnPath("/console/%2f%2fattacker", ["/console"])).toBe("/console");
    expect((await binding.fetch(request("/auth/callback", undefined, "POST")))!.status).toBe(405);
  } finally {
    await auth.close();
  }
});
