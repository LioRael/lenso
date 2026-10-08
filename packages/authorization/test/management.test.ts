import { describe, expect, test } from "bun:test";
import { createRoleManagement, RoleManagementError } from "../src/management";
import type { GrantAuthority, ManagementAction } from "../src/management";
import { memoryRoleStore, rbacPredicate } from "../src/rbac";
import type { Binding, Evaluation, RoleGraph } from "../src/types";

const actions = ["read", "write"] as const;
const scope = { type: "tenant", id: "a" };
const actor = { realmId: "realm", subjectId: "manager", kind: "human" };
const recipient = { ...actor, subjectId: "recipient" };
const read = { action: "read" as const, resourceType: "document", scope };
const write = { ...read, action: "write" as const };
const evaluation: Evaluation = { now: 100, signal: new AbortController().signal };
const graph: RoleGraph<"read" | "write"> = {
  roles: [
    { id: "base", scope, permissions: [read] },
    { id: "child", scope, permissions: [], inherits: ["base"] },
    { id: "writer", scope, permissions: [write] },
  ],
  bindings: [{ id: "source", principal: actor, scope, roleId: "child", expiresAt: 200 }],
};
const authority: GrantAuthority<"read" | "write"> = {
  scopes: [scope],
  permissions: [read, write],
  maxExpiresAt: 300,
};
const binding: Binding = {
  id: "new",
  principal: recipient,
  scope,
  roleId: "child",
  expiresAt: 150,
};
function setup(
  initial = graph,
  ceiling: GrantAuthority<"read" | "write"> | null = authority,
  authorize: (action: ManagementAction) => boolean = () => true,
) {
  const store = memoryRoleStore(initial, actions);
  const management = createRoleManagement({
    store,
    actions,
    authorize: (request) => authorize(request.action),
    grantAuthority: () => ceiling,
  });
  return { store, management };
}

describe("explicit role management", () => {
  test("use does not authorize grant, bind, edit, revoke or delegate", async () => {
    const { management } = setup(graph, authority, (action) => action === "use");
    const attempts = [
      () => management.createRole(actor, { id: "new", scope, permissions: [read] }, evaluation),
      () => management.bindRole(actor, binding, evaluation),
      () => management.editRole(actor, graph.roles[0]!, evaluation),
      () => management.revokeBinding(actor, scope, "source", evaluation),
      () => management.delegateRole(actor, "source", binding, evaluation),
    ];
    for (const attempt of attempts) await expect(attempt()).rejects.toThrow("Access denied");
  });

  test("requires mandatory trusted callbacks, masks application errors", async () => {
    expect(() =>
      createRoleManagement({ actions, store: memoryRoleStore(graph, actions) } as never),
    ).toThrow();
    const management = createRoleManagement({
      actions,
      store: memoryRoleStore(graph, actions),
      authorize: () => {
        throw new Error("private tenant policy");
      },
      grantAuthority: () => authority,
    });
    await expect(management.bindRole(actor, binding, evaluation)).rejects.toThrow("Access denied");
  });

  test("cannot edit an actor's bound role or its inherited base to self-elevate", async () => {
    const { management, store } = setup();
    const before = await store.read(evaluation);
    await expect(
      management.editRole(actor, { ...graph.roles[0]!, permissions: [read, write] }, evaluation),
    ).rejects.toThrow("Access denied");
    await expect(
      management.editRole(actor, { ...graph.roles[1]!, permissions: [write] }, evaluation),
    ).rejects.toThrow("Access denied");
    expect((await store.read(evaluation)).revision).toBe(before.revision);
  });

  test("unbound roles cannot stage permissions outside the grant ceiling", async () => {
    const { management } = setup({ ...graph, bindings: [] }, { ...authority, permissions: [read] });
    await expect(
      management.editRole(actor, { ...graph.roles[0]!, permissions: [write] }, evaluation),
    ).rejects.toThrow("Access denied");
    await expect(
      management.createRole(
        actor,
        { id: "staged", scope, permissions: [], inherits: ["writer"] },
        evaluation,
      ),
    ).rejects.toThrow("Access denied");
  });

  test("grant, bind and edit are independently authorized", async () => {
    const grantOnly = setup(graph, authority, (action) => action === "grant");
    const created = await grantOnly.management.createRole(
      actor,
      {
        id: "new-role",
        scope,
        permissions: [read],
      },
      evaluation,
    );
    expect((await grantOnly.store.read(evaluation)).graph.roles).toHaveLength(4);
    expect(Object.keys(created)).toEqual(["revision"]);
    await expect(grantOnly.management.bindRole(actor, binding, evaluation)).rejects.toThrow(
      "Access denied",
    );
    const editOnly = setup({ ...graph, bindings: [] }, authority, (action) => action === "edit");
    await editOnly.management.editRole(
      actor,
      {
        ...graph.roles[0]!,
        permissions: [read, write],
      },
      evaluation,
    );
    expect((await editOnly.store.read(evaluation)).graph.roles[0]!.permissions).toHaveLength(2);
  });

  test("edited descendants must satisfy the ceiling, including unbound roles", async () => {
    const { management } = setup(
      {
        roles: [
          { id: "base", scope, permissions: [] },
          { id: "child", scope, permissions: [write], inherits: ["base"] },
        ],
        bindings: [],
      },
      { ...authority, permissions: [read] },
    );
    await expect(
      management.editRole(actor, { id: "base", scope, permissions: [read] }, evaluation),
    ).rejects.toThrow("Access denied");
  });

  test("resource-limited authority cannot grant a whole resource type", async () => {
    const { management } = setup(graph, {
      ...authority,
      permissions: [{ ...read, resourceId: "one" }],
    });
    await expect(management.bindRole(actor, binding, evaluation)).rejects.toThrow("Access denied");
  });

  test("role edits cannot enlarge grants past affected binding expiry ceilings", async () => {
    for (const expiresAt of [undefined, 301]) {
      const { management } = setup({
        ...graph,
        bindings: [{ ...graph.bindings[0]!, principal: recipient, expiresAt }],
      });
      await expect(
        management.editRole(
          actor,
          {
            ...graph.roles[0]!,
            permissions: [read, write],
          },
          evaluation,
        ),
      ).rejects.toThrow("Access denied");
    }
    const { management, store } = setup({
      ...graph,
      bindings: [{ ...graph.bindings[0]!, principal: recipient, expiresAt: 200 }],
    });
    await management.editRole(
      actor,
      {
        ...graph.roles[0]!,
        permissions: [read, write],
      },
      evaluation,
    );
    expect((await store.read(evaluation)).graph.roles[0]!.permissions).toHaveLength(2);
  });

  test("cancellation after an adapter awaits prevents mutation", async () => {
    const controller = new AbortController();
    const store = memoryRoleStore(graph, actions);
    const before = await store.read(evaluation);
    const management = createRoleManagement({
      store,
      actions,
      authorize: () => true,
      grantAuthority: () => {
        controller.abort();
        return authority;
      },
    });
    await expect(
      management.bindRole(actor, binding, {
        ...evaluation,
        signal: controller.signal,
      }),
    ).rejects.toThrow("Access denied");
    expect((await store.read(evaluation)).revision).toBe(before.revision);
  });

  test("binding requires live authority, exact scope, permissions and bounded expiry", async () => {
    for (const ceiling of [
      null,
      { ...authority, maxExpiresAt: 100 },
      { ...authority, scopes: [{ type: "tenant", id: "other" }] },
      { ...authority, permissions: [write] },
    ]) {
      const { management } = setup(graph, ceiling);
      await expect(management.bindRole(actor, binding, evaluation)).rejects.toThrow(
        "Access denied",
      );
    }
    const { management, store } = setup();
    for (const expiresAt of [undefined, 100, 301, Infinity]) {
      await expect(
        management.bindRole(actor, { ...binding, expiresAt }, evaluation),
      ).rejects.toThrow("Access denied");
    }
    const result = await management.bindRole(actor, binding, evaluation);
    expect((await store.read(evaluation)).graph.bindings).toHaveLength(2);
    expect(Object.isFrozen(result)).toBe(true);
  });

  test("delegation cannot exceed source permissions, scope, expiry or identity", async () => {
    const { management, store } = setup();
    for (const proposed of [
      { ...binding, expiresAt: 201 },
      { ...binding, roleId: "writer" },
      { ...binding, scope: { type: "tenant", id: "b" } },
    ])
      await expect(management.delegateRole(actor, "source", proposed, evaluation)).rejects.toThrow(
        "Access denied",
      );
    await expect(
      management.delegateRole({ ...actor, realmId: "other" }, "source", binding, evaluation),
    ).rejects.toThrow("Access denied");
    await expect(management.delegateRole(actor, "missing", binding, evaluation)).rejects.toThrow(
      "Access denied",
    );
    await expect(
      management.delegateRole(actor, "source", binding, { ...evaluation, now: 200 }),
    ).rejects.toThrow("Access denied");
    await management.delegateRole(actor, "source", binding, evaluation);
    expect((await store.read(evaluation)).graph.bindings).toHaveLength(2);
  });

  test("authorized revocation immediately removes access", async () => {
    const { store, management } = setup();
    const predicate = rbacPredicate({ store, actions });
    const request = {
      principal: actor,
      action: "read" as const,
      context: {},
      resource: { type: "document", id: "1", scope },
    };
    expect(await predicate(request, evaluation)).toBe(true);
    await management.revokeBinding(actor, scope, "source", evaluation);
    expect(await predicate(request, evaluation)).toBe(false);
  });

  test("concurrent mutations use whole graph CAS and never retry conflicts", async () => {
    const { store, management } = setup();
    const before = await store.read(evaluation);
    const results = await Promise.allSettled([
      management.bindRole(actor, binding, evaluation),
      management.bindRole(actor, { ...binding, id: "second" }, evaluation),
    ]);
    expect(results.filter((r) => r.status === "fulfilled")).toHaveLength(1);
    const failure = results.find((r) => r.status === "rejected") as PromiseRejectedResult;
    expect(failure.reason).toBeInstanceOf(RoleManagementError);
    expect(failure.reason.code).toBe("CONFLICT");
    const after = await store.read(evaluation);
    expect(after.graph.bindings).toHaveLength(2);
    expect(after.revision).not.toBe(before.revision);
  });

  test("authorization receives immutable recipient and proposed role, preventing forbidden self-bind", async () => {
    const store = memoryRoleStore(graph, actions);
    const management = createRoleManagement({
      store,
      actions,
      authorize: (request) => {
        expect(Object.isFrozen(request.binding)).toBe(true);
        expect(Object.isFrozen(request.binding?.principal)).toBe(true);
        return request.binding?.principal.subjectId !== actor.subjectId;
      },
      grantAuthority: () => authority,
    });
    await expect(
      management.bindRole(actor, { ...binding, principal: actor, roleId: "writer" }, evaluation),
    ).rejects.toThrow("Access denied");
    expect(Object.keys(await management.bindRole(actor, binding, evaluation))).toEqual([
      "revision",
    ]);
  });

  test("revocation after authorize reads same role store cannot authorize a later-revision write", async () => {
    const store = memoryRoleStore(graph, actions);
    let entered!: () => void;
    let resume!: () => void;
    const started = new Promise<void>((resolve) => {
      entered = resolve;
    });
    const pause = new Promise<void>((resolve) => {
      resume = resolve;
    });
    const management = createRoleManagement({
      store,
      actions,
      authorize: async () => {
        const current = await store.read(evaluation);
        const granted = current.graph.bindings.some((item) => item.id === "source");
        entered();
        await pause;
        return granted;
      },
      grantAuthority: () => authority,
    });
    const mutation = management.bindRole(actor, binding, evaluation);
    const settled = mutation.then(
      () => "allowed",
      (error: unknown) => error,
    );
    await started;
    const before = await store.read(evaluation);
    await store.compareAndSwap(
      before.revision,
      {
        revision: crypto.randomUUID(),
        graph: { ...graph, bindings: [] },
      },
      evaluation,
    );
    resume();
    const outcome = await settled;
    expect(outcome).toBeInstanceOf(RoleManagementError);
    expect((outcome as RoleManagementError).code).toBe("CONFLICT");
    expect((await store.read(evaluation)).graph.bindings).toHaveLength(0);
  });
});
