import { expect, test } from "bun:test";
import { audience, createAuth, defineSource, realm } from "@lenso/auth";
import { createAuthorization } from "../src/core";
import { createRoleManagement } from "../src/management";
import { memoryRoleStore } from "../src/rbac";

test("management authorization must verify entry actor, not the proposed JSON principal", async () => {
  const auth = createAuth(
    realm(
      "staff",
      defineSource({
        verify: async (token: string) =>
          token === "reader-fixture"
            ? { status: "verified", subjectId: "reader" }
            : { status: "rejected" },
      }),
    ),
  );
  try {
    const access = auth.for(audience("roles:manage"));
    const actor = await access.required("reader-fixture");
    const scope = { type: "organization", id: "a" };
    const principal = { realmId: "staff", subjectId: "reader", kind: "user" };
    const read = { action: "read" as const, resourceType: "note", scope };
    const store = memoryRoleStore(
      {
        roles: [{ id: "reader", scope, permissions: [read] }],
        bindings: [],
      },
      ["read"],
    );
    let authorityCalls = 0;
    const guard = createAuthorization({
      actions: ["bind"],
      policies: [
        { evaluate: (facts) => (facts.principal?.subjectId === "admin" ? "allow" : "abstain") },
      ],
    });
    const management = createRoleManagement({
      store,
      actions: ["read"],
      authorize: async (request) => {
        await access.enforce(
          actor,
          request,
          async (verified) => {
            if (
              verified.principal.realmId !== request.actor.realmId ||
              verified.principal.subjectId !== request.actor.subjectId ||
              verified.principal.kind !== request.actor.kind
            )
              return false;
            return guard.can(
              {
                principal: {
                  realmId: verified.principal.realmId,
                  subjectId: verified.principal.subjectId,
                  kind: verified.principal.kind,
                },
                action: "bind",
                resource: { type: "authorization", id: request.targetId, scope: request.scope },
                context: {},
              },
              { signal: verified.signal },
            );
          },
          { signal: request.evaluation.signal },
        );
        return true;
      },
      grantAuthority: () => {
        authorityCalls++;
        return { permissions: [read], scopes: [scope], maxExpiresAt: 300 };
      },
    });
    const evaluation = { now: 100, signal: new AbortController().signal };
    const binding = { id: "new", principal, scope, roleId: "reader", expiresAt: 200 };
    await expect(management.bindRole(principal, binding, evaluation)).rejects.toThrow(
      "Access denied",
    );
    await expect(
      management.bindRole({ ...principal, subjectId: "admin" }, binding, evaluation),
    ).rejects.toThrow("Access denied");
    expect(authorityCalls).toBe(0);
    expect((await store.read(evaluation)).graph.bindings).toHaveLength(0);
  } finally {
    await auth.close();
  }
});
