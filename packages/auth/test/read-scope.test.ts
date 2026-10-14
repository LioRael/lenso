import { expect, test } from "bun:test";
import { audience, createAuth, defineSource, realm } from "../src/index";

function fixture() {
  const state = {
    now: 10,
    verifies: 0,
    rejected: false,
    allowed: true,
    subject: "fixture",
    kind: "user" as "user" | "guest",
    expiry: 10000,
  };
  const source = defineSource<string>({
    capabilities: { authoritative: true },
    async verify(_evidence, context) {
      state.verifies++;
      expect(context.authoritative).toBe(true);
      return state.rejected
        ? { status: "rejected" }
        : {
            status: "verified",
            subjectId: state.subject,
            kind: state.kind,
            session: { expiresAt: state.expiry, authoritative: true },
          };
    },
  });
  const auth = createAuth(realm("read-fixture", source), { now: () => state.now });
  const access = auth.for(audience("read")).requireSession({ authoritative: true });
  const controller = new AbortController();
  const begin = async (options: { signal?: AbortSignal; maxAgeMs?: number } = {}) => {
    const signal = options.signal ?? controller.signal;
    return access.beginRead!(await access.required("fixture", { signal }), { ...options, signal });
  };
  return { state, source, auth, access, begin, signal: controller.signal };
}

test("finite read reuses entry identity, checks each policy and refreshes once before disclosure", async () => {
  const { auth, access, state, begin, signal } = fixture();
  let policies = 0;
  const scoped = access.memberships(async (_subject, resource: string) =>
    resource === "own" && state.allowed ? { read: true } : null,
  );
  try {
    const scope = await begin();
    await scoped.enforceRead!(
      scope,
      "own",
      ({ membership }) => {
        policies++;
        return membership.read;
      },
      { signal },
    );
    await access.enforceRead!(
      scope,
      Object.freeze({ kind: "configuration" }),
      () => {
        policies++;
        return true;
      },
      { signal },
    );
    expect(state.verifies).toBe(1);
    const fresh = await scope.finish();
    expect(fresh).not.toBe(scope.actor);
    expect(state.verifies).toBe(2);
    expect(policies).toBe(4);
    await expect(access.enforceRead!(scope, "own", () => true, { signal })).rejects.toMatchObject({
      code: "UNAUTHORIZED",
    });
    await expect(scope.finish()).rejects.toMatchObject({ code: "UNAUTHORIZED" });
    await access.enforce(fresh, "own", () => true);
    await access.enforceAll(fresh, ["own"], () => true);
    await access.authorized("fixture", "own", () => true);
    expect(state.verifies).toBe(5);
  } finally {
    await auth.close();
  }
});

test("finish rejects authoritative revocation, identity/kind changes, policy and membership changes", async () => {
  for (const change of ["rejected", "subject", "kind", "membership", "policy"] as const) {
    const { auth, access, state, begin } = fixture();
    const scoped = access.memberships(async () => (state.allowed ? true : null));
    let policyAllowed = true;
    try {
      const scope = await begin();
      await scoped.enforceRead!(scope, "own", () => policyAllowed, { signal });
      if (change === "rejected") state.rejected = true;
      if (change === "subject") state.subject = "changed";
      if (change === "kind") state.kind = "guest";
      if (change === "membership") state.allowed = false;
      if (change === "policy") policyAllowed = false;
      await expect(scope.finish()).rejects.toMatchObject({
        code: change === "membership" || change === "policy" ? "FORBIDDEN" : "UNAUTHORIZED",
      });
      expect(state.verifies).toBe(2);
      await expect(scope.finish()).rejects.toMatchObject({ code: "UNAUTHORIZED" });
    } finally {
      await auth.close();
    }
  }
});

test("read lifetime rejects TTL, session expiry, cancellation, close and Auth close", async () => {
  for (const change of ["ttl", "expiry", "abort", "close", "auth-close"] as const) {
    const { auth, access, state, begin, signal } = fixture();
    const controller = new AbortController();
    if (change === "expiry") state.expiry = 20;
    const scope = await begin({ signal: controller.signal, maxAgeMs: 20 });
    if (change === "ttl") state.now = 30;
    if (change === "expiry") state.now = 20;
    if (change === "abort") controller.abort();
    if (change === "close") scope.close();
    if (change === "auth-close") await auth.close();
    await expect(
      access.enforceRead!(scope, "own", () => true, { signal: controller.signal }),
    ).rejects.toThrow();
    await expect(scope.finish()).rejects.toThrow();
    await auth.close();
  }
});

test("foreign runtime/audience, clones and mutable selectors are rejected", async () => {
  const { auth, access, source, begin, signal } = fixture();
  const other = createAuth(realm("read-fixture", source));
  try {
    const scope = await begin();
    for (const foreign of [other.for(audience("read")), auth.for(audience("other"))])
      await expect(
        foreign.enforceRead!(scope as never, "own", () => true, { signal }),
      ).rejects.toMatchObject({ code: "UNAUTHORIZED" });
    for (const forged of [{ ...scope }, JSON.parse(JSON.stringify(scope))])
      await expect(
        access.enforceRead!(forged, "own", () => true, { signal }),
      ).rejects.toMatchObject({ code: "UNAUTHORIZED" });
    for (const resource of [
      { kind: "configuration" },
      Object.freeze({ nested: {} }),
      Object.freeze({
        get kind() {
          return "configuration";
        },
      }),
    ])
      await expect(access.enforceRead!(scope, resource, () => true, { signal })).rejects.toThrow(
        "frozen flat selector",
      );
    scope.close();
  } finally {
    await auth.close();
    await other.close();
  }
});

test("unsupported sources cannot promise authoritative finish; age configuration is bounded", async () => {
  const auth = createAuth(
    realm(
      "unsupported",
      defineSource<string>({
        async verify() {
          return { status: "verified", subjectId: "fixture" };
        },
      }),
    ),
  );
  const access = auth.for(audience("read"));
  try {
    const signal = new AbortController().signal;
    const actor = await access.required("fixture", { signal });
    await expect(access.beginRead!(actor, { signal })).rejects.toThrow("Source cannot satisfy");
    for (const maxAgeMs of [0, 10001, NaN])
      await expect(access.beginRead!(actor, { signal, maxAgeMs })).rejects.toThrow("1 to 10000");
  } finally {
    await auth.close();
  }
});

test("scope rejects expiry across async policy and concurrent finish; selector remains fixed", async () => {
  const { auth, access, state, begin, signal } = fixture();
  try {
    const expired = await begin({ maxAgeMs: 20 });
    await expect(
      access.enforceRead!(
        expired,
        "own",
        async () => {
          await Promise.resolve();
          state.now = 30;
          return true;
        },
        { signal },
      ),
    ).rejects.toMatchObject({ code: "UNAUTHORIZED" });
    state.now = 10;
    const concurrent = await begin();
    let release!: () => void;
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    const pending = access.enforceRead!(
      concurrent,
      Object.freeze({ kind: "configuration" }),
      async () => {
        await gate;
        return true;
      },
      { signal },
    );
    await Promise.resolve();
    await expect(concurrent.finish()).rejects.toMatchObject({ code: "UNAUTHORIZED" });
    release();
    await expect(pending).rejects.toMatchObject({ code: "UNAUTHORIZED" });
  } finally {
    await auth.close();
  }
});

test("read adoption requires exact original request signal, fresh age, and consumes the actor once", async () => {
  const { auth, access, state } = fixture();
  try {
    const signal = new AbortController().signal;
    const actor = await access.required("fixture", { signal });
    await expect(
      access.beginRead!(actor, { signal: new AbortController().signal }),
    ).rejects.toMatchObject({ code: "UNAUTHORIZED" });
    await expect(access.beginRead!(actor, {} as never)).rejects.toMatchObject({
      code: "UNAUTHORIZED",
    });
    const scope = await access.beginRead!(actor, { signal });
    expect(state.verifies).toBe(1);
    await expect(access.beginRead!(actor, { signal })).rejects.toMatchObject({
      code: "UNAUTHORIZED",
    });
    scope.close();
    const old = await access.required("fixture", { signal });
    state.now += 21;
    await expect(access.beginRead!(old, { signal, maxAgeMs: 20 })).rejects.toMatchObject({
      code: "UNAUTHORIZED",
    });
    const unbound = await access.required("fixture");
    await expect(access.beginRead!(unbound, { signal })).rejects.toMatchObject({
      code: "UNAUTHORIZED",
    });
  } finally {
    await auth.close();
  }
});

test("every read operation requires its original request signal, including a generic scope handle", async () => {
  const { auth, access, begin, signal } = fixture();
  try {
    const scope: import("../src/index").ReadOnlyScope<string, string, string> = await begin();
    await expect(access.enforceRead!(scope, "own", () => true, {} as never)).rejects.toMatchObject({
      code: "UNAUTHORIZED",
    });
    await expect(
      access.enforceRead!(scope, "own", () => true, { signal: new AbortController().signal }),
    ).rejects.toMatchObject({ code: "UNAUTHORIZED" });
    const actor = await access.enforceRead!(scope, "own", () => true, { signal });
    expect(actor.audience).toBe("read");
    await scope.finish();
  } finally {
    await auth.close();
  }
});

test("slow entry verification cannot extend the short read snapshot lifetime", async () => {
  let now = 10;
  const auth = createAuth(
    realm(
      "slow-entry",
      defineSource<string>({
        capabilities: { authoritative: true },
        async verify() {
          await Promise.resolve();
          now = 31;
          return {
            status: "verified",
            subjectId: "fixture",
            session: { expiresAt: 1000, authoritative: true },
          };
        },
      }),
    ),
    { now: () => now },
  );
  const access = auth.for(audience("read"));
  try {
    const signal = new AbortController().signal;
    const actor = await access.required("fixture", { signal });
    await expect(access.beginRead!(actor, { signal, maxAgeMs: 20 })).rejects.toMatchObject({
      code: "UNAUTHORIZED",
    });
  } finally {
    await auth.close();
  }
});

test("derived read actors cannot reopen a scope and are cancelled on scope close", async () => {
  const { auth, access, begin, signal } = fixture();
  try {
    const scope = await begin();
    const derived = await access.enforceRead!(scope, "own", () => true, { signal });
    await expect(access.beginRead!(derived, { signal })).rejects.toMatchObject({
      code: "UNAUTHORIZED",
    });
    await access.enforce(derived, "own", () => true);
    scope.close();
    await expect(access.beginRead!(derived, { signal })).rejects.toMatchObject({
      code: "UNAUTHORIZED",
    });
    await expect(access.enforce(derived, "own", () => true)).rejects.toThrow();

    const second = await begin();
    await access.enforceRead!(second, "own", () => true, { signal });
    const fresh = await second.finish();
    second.close();
    await access.enforce(fresh, "own", () => true);
  } finally {
    await auth.close();
  }
});
