import { describe, expect, test } from "bun:test";
import { createAuthorization } from "../src/core";
import { all, any, attribute, predicate, relation } from "../src/conditions";
import { AuthorizationConfigurationError, AuthorizationError } from "../src/errors";
import type { Condition, Effect, Request, Resource, Rule } from "../src/types";

const scope = { type: "personal", id: "home" };
const resource: Resource = {
  type: "note",
  id: "one",
  scope,
  attributes: { public: true, state: "open" },
};
const principal = { realmId: "personal", subjectId: "alice", kind: "user" };
const request: Request<"read"> = { principal, action: "read", resource, context: {} };
const rule = (effect: "allow" | "deny", when: Condition = predicate(() => true)): Rule => ({
  id: effect,
  effect,
  actions: ["read"],
  resourceType: "note",
  when,
});

describe("conflict matrix", () => {
  const effects: Effect[] = ["allow", "deny", "abstain"];
  for (const left of effects)
    for (const right of effects) {
      test(`${left} and ${right}, both orders`, async () => {
        const expected =
          left === "deny" || right === "deny"
            ? "deny"
            : left === "allow" || right === "allow"
              ? "allow"
              : "deny";
        for (const pair of [
          [left, right],
          [right, left],
        ]) {
          const engine = createAuthorization({
            actions: ["read"],
            policies: pair.map((effect) => ({ evaluate: () => effect })),
          });
          expect((await engine.check(request)).effect).toBe(expected);
        }
      });
    }
});

test("applicable rule deny wins; nonmatching deny does not", async () => {
  const engine = createAuthorization({ actions: ["read"], rules: [rule("allow"), rule("deny")] });
  expect((await engine.check(request)).code).toBe("EXPLICIT_DENY");
  expect(
    await createAuthorization({
      actions: ["read"],
      rules: [rule("allow"), { ...rule("deny"), resourceId: "other" }],
    }).can(request),
  ).toBe(true);
  expect(await createAuthorization({ actions: ["read"] }).can(request)).toBe(false);
});

test("all narrows a grant; any is explicit OR; errors are not hidden in OR", async () => {
  const granted = predicate(() => true);
  const restricted = attribute("resource", "state", "equals", "closed");
  const make = (when: Condition) =>
    createAuthorization({ actions: ["read"], rules: [rule("allow", when)] });
  expect(await make(all(granted, restricted)).can(request)).toBe(false);
  expect(await make(any(granted, restricted)).can(request)).toBe(true);
  expect(
    (
      await make(
        any(
          granted,
          predicate(() => {
            throw new Error("secret");
          }),
        ),
      ).check(request)
    ).code,
  ).toBe("EVALUATION_FAILED");
  expect(await make(attribute("resource", "missing", "equals", null)).can(request)).toBe(false);
  expect(await make(attribute("resource", "state", "in", ["open", "closed"])).can(request)).toBe(
    true,
  );
});

test("public anonymous is explicit and does not bypass configured identity", async () => {
  const publicRule = rule("allow", attribute("resource", "public", "equals", true));
  expect(
    await createAuthorization({ actions: ["read"], rules: [publicRule] }).can({
      ...request,
      principal: null,
    }),
  ).toBe(true);
  expect(
    await createAuthorization({
      actions: ["read"],
      identity: { required: true },
      rules: [publicRule],
    }).can({ ...request, principal: null }),
  ).toBe(false);
  expect(
    await createAuthorization({ actions: ["read"], rules: [publicRule] }).can({
      ...request,
      principal: undefined,
    } as unknown as Request),
  ).toBe(false);
});

test("identity, audience and credential ceilings run before custom policy", async () => {
  let calls = 0;
  const engine = createAuthorization<string>({
    actions: ["read", "write"],
    identity: {
      required: true,
      realms: ["personal"],
      audiences: ["notes"],
      credentialRequired: true,
    },
    policies: [
      {
        evaluate() {
          calls++;
          return "allow";
        },
      },
    ],
    clock: () => 10,
  });
  const credential = {
    permissions: [{ action: "read", resourceType: "note", scope }],
    expiresAt: 20,
  };
  const valid = { ...request, audience: "notes", credential };
  for (const input of [
    { ...valid, principal: null },
    { ...valid, principal: { ...principal, realmId: "console" } },
    { ...valid, audience: "console" },
    { ...valid, credential: undefined },
    { ...valid, credential: { ...credential, expiresAt: 10 } },
    { ...valid, action: "write" },
    { ...valid, resource: { ...resource, scope: { ...scope, id: "other" } } },
    { ...valid, credential: { permissions: [] } },
  ])
    expect((await engine.check(input)).code).toBe("BOUNDARY_DENIED");
  expect(calls).toBe(0);
  expect(await engine.can(valid)).toBe(true);
  expect(calls).toBe(1);
});

test("scope is exact, not a prefix or tenant magic; explicit cross-org policy allowed", async () => {
  const engine = createAuthorization({
    actions: ["read"],
    rules: [{ ...rule("allow"), scope: { type: "organization", id: "b" } }],
    boundaries: [(facts) => facts.context.approvedCrossOrganization === true],
  });
  const cross = {
    ...request,
    resource: { ...resource, scope: { type: "organization", id: "b" } },
    context: { approvedCrossOrganization: true, sourceOrganization: "a" },
  };
  expect(await engine.can(cross)).toBe(true);
  expect(await engine.can({ ...cross, context: {} })).toBe(false);
  expect(
    await engine.can({
      ...cross,
      resource: { ...cross.resource, scope: { type: "organization", id: "b/child" } },
    }),
  ).toBe(false);
});

test("relation resolver is direct and optional; missing or failing resolver denies", async () => {
  const options = {
    actions: ["read"],
    rules: [rule("allow", any(relation("owner"), relation("shared-with")))],
  };
  expect((await createAuthorization(options).check(request)).code).toBe("EVALUATION_FAILED");
  const engine = createAuthorization({
    ...options,
    relations: {
      check: (subject, name, object) =>
        subject.subjectId === "alice" && object.id === "one" && name === "shared-with",
    },
  });
  expect(await engine.can(request)).toBe(true);
  expect(await engine.can({ ...request, principal: { ...principal, subjectId: "bob" } })).toBe(
    false,
  );
  expect(await engine.can({ ...request, principal: null })).toBe(false);
});

test("unknown actions, malformed extensions and resolver target substitution never allow", async () => {
  expect(
    (
      await createAuthorization<string>({ actions: ["read"], rules: [rule("allow")] }).check({
        ...request,
        action: "unknown",
      })
    ).code,
  ).toBe("UNKNOWN_ACTION");
  expect(
    (
      await createAuthorization({
        actions: ["read"],
        policies: [{ evaluate: () => true as unknown as Effect }],
      }).check(request)
    ).code,
  ).toBe("EVALUATION_FAILED");
  expect(
    await createAuthorization({
      actions: ["read"],
      rules: [rule("allow")],
      resolveResource: (item) => ({ ...item, id: "another" }),
    }).can(request),
  ).toBe(false);
  expect(() =>
    createAuthorization({ actions: ["read"], rules: [{ ...rule("allow"), actions: ["unknown"] }] }),
  ).toThrow(AuthorizationConfigurationError);
});

test("condition cycles, empty branches and excessive depth rejected", () => {
  const cyclic = { kind: "all", conditions: [] } as unknown as {
    kind: "all";
    conditions: Condition[];
  };
  cyclic.conditions.push(cyclic);
  for (const when of [cyclic, all(), any()]) {
    expect(() => createAuthorization({ actions: ["read"], rules: [rule("allow", when)] })).toThrow(
      AuthorizationConfigurationError,
    );
  }
  let deep: Condition = predicate(() => true);
  for (let i = 0; i < 40; i++) deep = all(deep);
  expect(() => createAuthorization({ actions: ["read"], rules: [rule("allow", deep)] })).toThrow(
    AuthorizationConfigurationError,
  );
});

test("snapshot is immutable across awaits without freezing borrowed facts", async () => {
  let resume!: () => void;
  let entered!: () => void;
  const started = new Promise<void>((resolve) => {
    entered = resolve;
  });
  const pause = new Promise<void>((resolve) => {
    resume = resolve;
  });
  const mutable = { ...request, resource: { ...resource, attributes: { state: "closed" } } };
  const engine = createAuthorization({
    actions: ["read"],
    rules: [
      rule(
        "allow",
        predicate(async (facts) => {
          entered();
          await pause;
          return facts.resource.attributes?.state === "open";
        }),
      ),
    ],
  });
  const result = engine.can(mutable);
  await started;
  mutable.resource.attributes.state = "open";
  resume();
  expect(await result).toBe(false);
  expect(Object.isFrozen(mutable.resource)).toBe(false);
});

test("timeouts, abort, late allow and throw are safe refusals", async () => {
  let captured: AbortSignal | undefined;
  let finish!: (value: Effect) => void;
  const engine = createAuthorization({
    actions: ["read"],
    timeoutMs: 5,
    policies: [
      {
        evaluate: (_facts, evaluation) => {
          captured = evaluation.signal;
          return new Promise<Effect>((resolve) => {
            finish = resolve;
          });
        },
      },
    ],
  });
  expect((await engine.check(request)).code).toBe("TIMEOUT");
  expect(captured?.aborted).toBe(true);
  finish("allow");
  const controller = new AbortController();
  controller.abort("private reason");
  expect((await engine.check(request, { signal: controller.signal })).code).toBe("CANCELLED");
  expect(
    (
      await createAuthorization({
        actions: ["read"],
        policies: [
          {
            evaluate: () => {
              throw new Error("secret");
            },
          },
        ],
      }).check(request)
    ).code,
  ).toBe("EVALUATION_FAILED");
  await expect(engine.enforce(request, { signal: controller.signal })).rejects.toThrow(
    "Access denied.",
  );
});

test("explain requires explicit authorized management action and returns only ordinal paths", async () => {
  const gate = { type: "authorization", id: "instance", scope: { type: "platform", id: "app" } };
  const engine = createAuthorization({
    actions: ["read", "explain"],
    explain: { action: "explain", resource: gate },
    rules: [
      { ...rule("allow"), id: "secret-tenant-rule" },
      {
        id: "admin",
        effect: "allow",
        actions: ["explain"],
        resourceType: "authorization",
        when: predicate((facts) => facts.principal?.subjectId === "admin"),
      },
    ],
  });
  const manager = {
    ...request,
    action: "explain",
    resource: gate,
    principal: { ...principal, subjectId: "admin" },
  };
  await expect(engine.explain(request, request)).rejects.toThrow(AuthorizationError);
  await expect(engine.explain({ ...manager, principal }, request)).rejects.toThrow(
    AuthorizationError,
  );
  const explanation = await engine.explain(manager, request);
  expect(explanation.paths).toEqual(["rules/0/allow"]);
  expect(JSON.stringify(explanation)).not.toContain("secret-tenant-rule");
  expect(JSON.stringify(await engine.check(request))).not.toContain("paths");
});

test("structured attribute resolution stays inside credential limits and before boundaries", async () => {
  let resolved = 0;
  const engine = createAuthorization<string>({
    actions: ["read", "write"],
    identity: { credentialRequired: true },
    attributes: {
      resolve() {
        resolved++;
        return { principal: { approved: true }, resource: { state: "reviewed" } };
      },
    },
    boundaries: [(facts) => facts.principal?.attributes?.approved === true],
    rules: [rule("allow", attribute("resource", "state", "equals", "reviewed"))],
  });
  const credential = { permissions: [{ action: "read", resourceType: "note", scope }] };
  expect(await engine.can({ ...request, credential })).toBe(true);
  expect(await engine.can({ ...request, action: "write", credential })).toBe(false);
  expect(resolved).toBe(1);
  expect(
    (
      await createAuthorization({
        actions: ["read"],
        policies: [{ evaluate: () => "allow" }],
        attributes: {
          resolve: () => {
            throw new Error("private");
          },
        },
      }).check(request)
    ).code,
  ).toBe("EVALUATION_FAILED");
  expect(
    (
      await createAuthorization({
        actions: ["read"],
        policies: [{ evaluate: () => "allow" }],
        attributes: { resolve: () => ({ principal: { approved: true } }) },
      }).check({ ...request, principal: null })
    ).code,
  ).toBe("EVALUATION_FAILED");
});
