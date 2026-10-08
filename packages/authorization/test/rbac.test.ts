import { describe, expect, test } from "bun:test";
import {
  effectiveRolePermissions,
  memoryRoleStore,
  rbacPolicy,
  rbacPredicate,
  validateRoleGraph,
} from "../src/rbac";
import type { Evaluation, RoleGraph } from "../src/types";

const actions = ["read", "write"] as const;
const scope = { type: "tenant", id: "a" };
const other = { type: "tenant", id: "b" };
const principal = { realmId: "realm-a", subjectId: "user", kind: "human" };
const evaluation: Evaluation = { now: 100, signal: new AbortController().signal };
const read = { action: "read" as const, resourceType: "document", scope };
const graph: RoleGraph<"read" | "write"> = {
  roles: [
    { id: "reader", scope, permissions: [read] },
    { id: "member", scope, permissions: [], inherits: ["reader"] },
  ],
  bindings: [{ id: "binding", principal, roleId: "member", scope, expiresAt: 200 }],
};
const request = {
  principal,
  action: "read" as const,
  resource: { type: "document", id: "1", scope },
  context: {},
};

describe("scoped RBAC", () => {
  test("inherits permissions only in exact scope and exact identity", async () => {
    const predicate = rbacPredicate({ graph, actions });
    expect(await predicate(request, evaluation)).toBe(true);
    expect(
      await predicate({ ...request, resource: { ...request.resource, scope: other } }, evaluation),
    ).toBe(false);
    for (const changed of [
      { ...principal, realmId: "realm-b" },
      { ...principal, kind: "service" },
      { ...principal, subjectId: "someone-else" },
    ])
      expect(await predicate({ ...request, principal: changed }, evaluation)).toBe(false);
    expect(await predicate({ ...request, principal: null }, evaluation)).toBe(false);
    expect(await predicate({ ...request, action: "write" }, evaluation)).toBe(false);
    expect(await predicate(request, { ...evaluation, now: 200 })).toBe(false);
    expect(effectiveRolePermissions(graph, "member", scope)).toEqual([read]);
  });

  test("reads current snapshot every evaluation, so revocation takes effect", async () => {
    const store = memoryRoleStore(graph, actions);
    const policy = rbacPolicy({ store, actions });
    expect(await policy.evaluate(request, evaluation)).toBe("allow");
    const before = await store.read(evaluation);
    expect(
      await store.compareAndSwap(
        before.revision,
        {
          revision: crypto.randomUUID(),
          graph: { ...graph, bindings: [] },
        },
        evaluation,
      ),
    ).toBe(true);
    expect(await policy.evaluate(request, evaluation)).toBe("abstain");
    expect(await store.compareAndSwap(before.revision, before, evaluation)).toBe(false);
  });

  test("snapshots detach caller data and are deeply immutable", async () => {
    const input = structuredClone(graph);
    const store = memoryRoleStore(input, actions);
    (input.roles as unknown[]).length = 0;
    const snapshot = await store.read(evaluation);
    expect(snapshot.graph.roles).toHaveLength(2);
    expect(Object.isFrozen(snapshot.graph.roles[0]!.permissions[0]!.scope)).toBe(true);
    expect(Object.isFrozen(snapshot.graph.bindings[0]!.principal)).toBe(true);
    expect(
      await store.compareAndSwap(
        snapshot.revision,
        {
          revision: crypto.randomUUID(),
          graph: { roles: [], bindings: [] },
        },
        { ...evaluation, signal: AbortSignal.abort() },
      ),
    ).toBe(false);
  });

  test("memory snapshots reject mutable containers, cycles and unchanged revisions", async () => {
    expect(() =>
      memoryRoleStore(
        {
          ...graph,
          bindings: [
            {
              ...graph.bindings[0]!,
              principal: { ...principal, attributes: { date: new Date() } },
            },
          ],
        },
        actions,
      ),
    ).toThrow();
    const cyclic: Record<string, unknown> = {};
    cyclic.self = cyclic;
    expect(() =>
      memoryRoleStore(
        {
          ...graph,
          bindings: [
            {
              ...graph.bindings[0]!,
              principal: { ...principal, attributes: cyclic },
            },
          ],
        },
        actions,
      ),
    ).toThrow();
    const store = memoryRoleStore(graph, actions);
    const current = await store.read(evaluation);
    expect(() => store.compareAndSwap(current.revision, current, evaluation)).toThrow();
    expect((await store.read(evaluation)).revision).toBe(current.revision);
  });

  test("rejects cycles, unknown inheritance and cross-scope inheritance", () => {
    const invalid: RoleGraph[] = [
      { ...graph, roles: [{ id: "loop", scope, permissions: [], inherits: ["loop"] }] },
      {
        roles: [
          { id: "a", scope, permissions: [], inherits: ["b"] },
          { id: "b", scope, permissions: [], inherits: ["a"] },
        ],
        bindings: [],
      },
      {
        roles: [
          { id: "a", scope, permissions: [], inherits: ["b"] },
          { id: "b", scope: other, permissions: [] },
        ],
        bindings: [],
      },
      { ...graph, roles: [{ id: "member", scope, permissions: [], inherits: ["missing"] }] },
    ];
    for (const candidate of invalid)
      expect(() => validateRoleGraph(candidate, actions)).toThrow("Invalid role graph");
  });

  test("rejects unknown actions, duplicate keys, cross-scope permissions and unknown bound roles", () => {
    const invalid: RoleGraph[] = [
      { ...graph, roles: [...graph.roles, graph.roles[0]!] },
      { ...graph, bindings: [...graph.bindings, graph.bindings[0]!] },
      { ...graph, roles: [{ id: "reader", scope, permissions: [read, read] }] },
      { ...graph, roles: [{ id: "reader", scope, permissions: [{ ...read, action: "unknown" }] }] },
      { ...graph, roles: [{ id: "reader", scope, permissions: [{ ...read, scope: other }] }] },
      { ...graph, bindings: [{ ...graph.bindings[0]!, roleId: "missing" }] },
      { ...graph, bindings: [{ ...graph.bindings[0]!, expiresAt: Infinity }] },
    ];
    for (const candidate of invalid) expect(() => validateRoleGraph(candidate, actions)).toThrow();
    expect(() => validateRoleGraph(graph, ["read", "read"])).toThrow();
    expect(() =>
      validateRoleGraph(
        {
          roles: Array.from({ length: 513 }, (_, i) => ({ id: String(i), scope, permissions: [] })),
          bindings: [],
        },
        actions,
      ),
    ).toThrow();
  });

  test("resource-specific grants do not widen and store failures deny", async () => {
    const predicate = rbacPredicate({
      actions,
      graph: {
        roles: [{ id: "reader", scope, permissions: [{ ...read, resourceId: "1" }] }],
        bindings: [{ ...graph.bindings[0]!, roleId: "reader" }],
      },
    });
    expect(await predicate(request, evaluation)).toBe(true);
    expect(
      await predicate({ ...request, resource: { ...request.resource, id: "2" } }, evaluation),
    ).toBe(false);
    const failed = rbacPredicate({
      actions,
      store: {
        read() {
          throw new Error("private tenant details");
        },
        compareAndSwap: () => false,
      },
    });
    await expect(failed(request, evaluation)).rejects.toThrow("Invalid role graph");
  });
});
