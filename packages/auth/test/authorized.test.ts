import { expect, test } from "bun:test";
import { audience, createAuth, defineSource, realm } from "../src/index";

test("authorized authenticates once for its own boundary; returned actors retain audience and fresh enforce", async () => {
  let verifies = 0,
    rejected = false;
  const auth = createAuth(
    realm(
      "combined",
      defineSource<string>({
        async verify(evidence) {
          verifies++;
          return rejected ? { status: "rejected" } : { status: "verified", subjectId: evidence };
        },
      }),
    ),
  );
  const access = auth
    .for(audience("read"))
    .memberships(async (_subject, tenant: string) =>
      tenant === "own" ? { role: "reader" } : null,
    );
  try {
    const actor = await access.authorized(
      "fixture",
      "own",
      ({ membership }) => membership.role === "reader",
    );
    expect(verifies).toBe(1);
    expect(Object.isFrozen(actor)).toBe(true);
    await expect(
      auth.for(audience("other")).enforce(actor as never, {}, () => true),
    ).rejects.toMatchObject({ code: "UNAUTHORIZED" });
    await expect(access.enforce({ ...actor } as never, "own", () => true)).rejects.toMatchObject({
      code: "UNAUTHORIZED",
    });
    await access.enforce(actor, "own", () => true);
    expect(verifies).toBe(2);
    await expect(access.authorized("fixture", "other", () => true)).rejects.toMatchObject({
      code: "FORBIDDEN",
    });
    rejected = true;
    await expect(access.enforce(actor, "own", () => true)).rejects.toMatchObject({
      code: "UNAUTHORIZED",
    });
    await expect(access.authorized("fixture", "own", () => true)).rejects.toMatchObject({
      code: "UNAUTHORIZED",
    });
  } finally {
    await auth.close();
  }
});

test("authorized checks abort and session expiry after asynchronous policy", async () => {
  let now = 10;
  const auth = createAuth(
    realm(
      "combined",
      defineSource<string>({
        async verify(evidence) {
          return { status: "verified", subjectId: evidence, session: { expiresAt: 20 } };
        },
      }),
    ),
    { now: () => now },
  );
  const access = auth.for(audience("read"));
  try {
    const controller = new AbortController();
    await expect(
      access.authorized(
        "fixture",
        {},
        async () => {
          await Promise.resolve();
          controller.abort(new Error("fixture abort"));
          return true;
        },
        { signal: controller.signal },
      ),
    ).rejects.toThrow("fixture abort");
    await expect(
      access.authorized("fixture", {}, async () => {
        await Promise.resolve();
        now = 20;
        return true;
      }),
    ).rejects.toMatchObject({ code: "UNAUTHORIZED" });
  } finally {
    await auth.close();
  }
});

test("enforceAll verifies one explicit stage and checks every current membership and policy", async () => {
  let verifies = 0,
    rejected = false;
  const memberships: object[] = [],
    policies: object[] = [];
  const auth = createAuth(
    realm(
      "stages",
      defineSource<string>({
        async verify(evidence) {
          verifies++;
          return rejected ? { status: "rejected" } : { status: "verified", subjectId: evidence };
        },
      }),
    ),
  );
  const access = auth
    .for(audience("read"))
    .memberships(async (_subject, resource: { allowed: boolean }) => {
      memberships.push(resource);
      return resource.allowed ? { role: "reader" } : null;
    });
  try {
    const actor = await access.required("fixture");
    const first = { allowed: true },
      second = { allowed: true };
    verifies = 0;
    await access.enforceAll(actor, [first, second] as const, ({ resource, membership }) => {
      policies.push(resource);
      return membership.role === "reader";
    });
    expect(verifies).toBe(1);
    expect(memberships).toEqual([first, second]);
    expect(policies[0]).toBe(first);
    expect(policies[1]).toBe(second);
    await expect(access.enforceAll(actor, [], () => true)).rejects.toThrow(
      "At least one authorization resource",
    );
    await expect(
      access.enforceAll(actor, [first, { allowed: false }], () => true),
    ).rejects.toMatchObject({ code: "FORBIDDEN" });
    await expect(
      access.enforceAll(actor, [first, second], ({ resource }) => resource !== second),
    ).rejects.toMatchObject({ code: "FORBIDDEN" });
    rejected = true;
    await expect(access.enforceAll(actor, [first, second], () => true)).rejects.toMatchObject({
      code: "UNAUTHORIZED",
    });
  } finally {
    await auth.close();
  }
});

test("enforceAll stops an explicit stage on asynchronous policy abort or expiry", async () => {
  let now = 10,
    calls = 0;
  const auth = createAuth(
    realm(
      "stages",
      defineSource<string>({
        async verify(evidence) {
          return { status: "verified", subjectId: evidence, session: { expiresAt: 20 } };
        },
      }),
    ),
    { now: () => now },
  );
  const access = auth.for(audience("read"));
  try {
    const actor = await access.required("fixture");
    const controller = new AbortController();
    await expect(
      access.enforceAll(
        actor,
        [1, 2] as const,
        async () => {
          calls++;
          await Promise.resolve();
          controller.abort(new Error("stage abort"));
          return true;
        },
        { signal: controller.signal },
      ),
    ).rejects.toThrow("stage abort");
    expect(calls).toBe(1);
    calls = 0;
    await expect(
      access.enforceAll(actor, [1, 2] as const, async () => {
        calls++;
        await Promise.resolve();
        now = 20;
        return true;
      }),
    ).rejects.toMatchObject({ code: "UNAUTHORIZED" });
    expect(calls).toBe(1);
  } finally {
    await auth.close();
  }
});
