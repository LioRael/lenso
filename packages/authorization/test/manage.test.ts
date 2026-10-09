import { expect, test } from "bun:test";
import { createAuthorizationInspection } from "../src/manage";
import { memoryRoleStore } from "../src/rbac";

const scope = { type: "workspace", id: "a" };
const other = { type: "workspace", id: "b" };
const principal = { realmId: "host", subjectId: "alice", kind: "user" };

test("inspection exposes only an authorized exact scope, not the repository", async () => {
  const store = memoryRoleStore(
    {
      roles: [
        { id: "reader", scope, permissions: [] },
        { id: "reader", scope: other, permissions: [] },
      ],
      bindings: [
        { id: "a", principal, roleId: "reader", scope },
        {
          id: "private-a",
          principal: { ...principal, subjectId: "private" },
          roleId: "reader",
          scope,
        },
        { id: "b", principal, roleId: "reader", scope: other },
      ],
    },
    ["read"],
  );
  const inspection = createAuthorizationInspection({
    store,
    actions: ["read"],
    authorize: (caller: object, selected) => caller === principal && selected.id === "a",
    authorizeBinding: (_caller, binding) => binding.principal.subjectId === "alice",
  });
  const evaluation = { now: Date.now(), signal: new AbortController().signal };
  const result = await inspection.inspect(scope, principal, evaluation);
  expect(result.graph.roles.map((role) => role.scope.id)).toEqual(["a"]);
  expect(result.graph.bindings.map((binding) => binding.id)).toEqual(["a"]);
  expect(Object.keys(inspection)).toEqual(["inspect"]);
  await expect(inspection.inspect(other, principal, evaluation)).rejects.toThrow();
  await expect(inspection.inspect(scope, { ...principal }, evaluation)).rejects.toThrow();
});

test("inspection revalidates authority after a repository await", async () => {
  let allowed = true;
  const store = memoryRoleStore({ roles: [], bindings: [] }, ["read"]);
  const inspection = createAuthorizationInspection({
    actions: ["read"],
    authorize: () => allowed,
    authorizeBinding: () => true,
    store: {
      async read(evaluation) {
        const result = await store.read(evaluation);
        allowed = false;
        return result;
      },
      compareAndSwap: store.compareAndSwap,
    },
  });
  await expect(
    inspection.inspect(scope, principal, {
      now: Date.now(),
      signal: new AbortController().signal,
    }),
  ).rejects.toThrow();
});
