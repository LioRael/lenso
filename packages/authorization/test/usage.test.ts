import { expect, test } from "bun:test";
import {
  all,
  any,
  attribute,
  createAuthorization,
  predicate,
  rbacPolicy,
  rbacPredicate,
  relation,
} from "../src";
import type { Request, RoleGraph } from "../src";

const scope = { type: "personal", id: "home" };
const principal = { realmId: "notes", subjectId: "alice", kind: "user" };
const graph: RoleGraph = {
  roles: [{ id: "reader", scope, permissions: [{ action: "read", resourceType: "note", scope }] }],
  bindings: [{ id: "alice-reader", principal, roleId: "reader", scope }],
};
const request: Request<"read"> = {
  principal,
  action: "read",
  context: {},
  resource: { type: "note", id: "one", scope, attributes: { state: "open" } },
};

test("example 1: standalone RBAC, no organization/database/Lenso", async () => {
  const authorization = createAuthorization({
    actions: ["read"],
    policies: [rbacPolicy({ actions: ["read"], graph })],
  });
  expect(await authorization.can(request)).toBe(true);
  expect(
    await authorization.can({ ...request, principal: { ...principal, subjectId: "bob" } }),
  ).toBe(false);
});

test("example 2: RBAC AND resource condition, not independent OR grants", async () => {
  const authorization = createAuthorization({
    actions: ["read"],
    rules: [
      {
        id: "reader-open",
        effect: "allow",
        actions: ["read"],
        resourceType: "note",
        when: all(
          predicate(rbacPredicate({ actions: ["read"], graph })),
          attribute("resource", "state", "equals", "open"),
        ),
      },
    ],
  });
  expect(await authorization.can(request)).toBe(true);
  expect(
    await authorization.can({
      ...request,
      resource: { ...request.resource, attributes: { state: "closed" } },
    }),
  ).toBe(false);
});

test("example 3: organization member OR resource sharing, explicit cross-organization sharing", async () => {
  const organization = {
    type: "organization",
    id: "team-a",
    scope: { type: "platform", id: "app" },
  };
  const authorization = createAuthorization({
    actions: ["read"],
    relations: {
      check: (subject, name, target) =>
        subject.subjectId === "alice" &&
        ((name === "member" && target.id === "team-a") ||
          (name === "shared-with" && target.id === "shared")),
    },
    rules: [
      {
        id: "member-or-shared",
        effect: "allow",
        actions: ["read"],
        resourceType: "note",
        when: any(relation("member", organization), relation("shared-with")),
      },
    ],
  });
  expect(await authorization.can(request)).toBe(true);
  expect(
    await authorization.can({
      ...request,
      resource: {
        ...request.resource,
        id: "shared",
        scope: { type: "organization", id: "team-b" },
      },
    }),
  ).toBe(true);
  expect(
    await authorization.can({ ...request, principal: { ...principal, subjectId: "bob" } }),
  ).toBe(false);
});

test("example 4: custom policy still obeys verified credential ceiling", async () => {
  const authorization = createAuthorization({
    actions: ["read", "write"],
    identity: { credentialRequired: true },
    policies: [
      { evaluate: (facts) => (facts.principal?.kind === "service" ? "allow" : "abstain") },
    ],
  });
  const agent = {
    ...request,
    principal: { ...principal, kind: "service" },
    credential: { permissions: [{ action: "read" as const, resourceType: "note", scope }] },
  };
  expect(await authorization.can(agent)).toBe(true);
  expect(await authorization.can({ ...agent, action: "write" })).toBe(false);
});

test("resolver failure and unknown roles cannot be hidden behind another allow", async () => {
  const authorization = createAuthorization({
    actions: ["read"],
    policies: [
      { evaluate: () => "allow" },
      rbacPolicy({
        actions: ["read"],
        store: {
          read: () => ({
            revision: "broken",
            graph: { roles: [], bindings: [{ id: "bad", principal, roleId: "unknown", scope }] },
          }),
          compareAndSwap: () => false,
        },
      }),
    ],
  });
  expect((await authorization.check(request)).code).toBe("EVALUATION_FAILED");
});
