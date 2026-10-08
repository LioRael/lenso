import type {
  Attributes,
  Permission,
  Policy,
  Predicate,
  Principal,
  Resource,
  RoleGraph,
  RoleSnapshot,
  RoleStore,
  Scope,
} from "./types";
import { snapshot } from "./snapshot";
import { sameScope } from "./conditions";

export class RoleGraphError extends Error {
  constructor() {
    super("Invalid role graph");
    this.name = "RoleGraphError";
  }
}

const MAX_ROLES = 512;
const MAX_BINDINGS = 4096;
const MAX_ITEMS = 512;
const text = (value: unknown): value is string =>
  typeof value === "string" && value.length > 0 && value.length <= 256;
const scopeValid = (scope: Scope): boolean => !!scope && text(scope.type) && text(scope.id);
export { sameScope } from "./conditions";
export const samePrincipal = (a: Principal, b: Principal): boolean =>
  a.realmId === b.realmId && a.subjectId === b.subjectId && a.kind === b.kind;
const principalValid = (p: Principal): boolean =>
  !!p && text(p.realmId) && text(p.subjectId) && text(p.kind);
const key = (scope: Scope, id: string): string => JSON.stringify([scope.type, scope.id, id]);
const permissionKey = (p: Permission): string =>
  JSON.stringify([p.action, p.resourceType, p.scope.type, p.scope.id, p.resourceId ?? null]);

export function permissionCovers(ceiling: Permission, permission: Permission): boolean {
  return (
    ceiling.action === permission.action &&
    ceiling.resourceType === permission.resourceType &&
    sameScope(ceiling.scope, permission.scope) &&
    (ceiling.resourceId === undefined || ceiling.resourceId === permission.resourceId)
  );
}

export function validateRoleGraph<A extends string>(
  graph: RoleGraph<A>,
  actions: readonly A[],
): void {
  const fail = (): never => {
    throw new RoleGraphError();
  };
  if (
    !Array.isArray(actions) ||
    actions.length === 0 ||
    actions.length > MAX_ITEMS ||
    actions.some((a) => !text(a)) ||
    new Set(actions).size !== actions.length
  )
    fail();
  if (
    !graph ||
    !Array.isArray(graph.roles) ||
    !Array.isArray(graph.bindings) ||
    graph.roles.length > MAX_ROLES ||
    graph.bindings.length > MAX_BINDINGS
  )
    fail();
  const known = new Set(actions);
  const roles = new Map<string, RoleGraph<A>["roles"][number]>();
  for (const role of graph.roles) {
    if (
      !role ||
      !text(role.id) ||
      !scopeValid(role.scope) ||
      !Array.isArray(role.permissions) ||
      role.permissions.length > MAX_ITEMS ||
      (role.inherits !== undefined &&
        (!Array.isArray(role.inherits) || role.inherits.length > MAX_ROLES))
    )
      fail();
    const k = key(role.scope, role.id);
    if (roles.has(k)) fail();
    roles.set(k, role);
    const permissions = new Set<string>();
    for (const p of role.permissions) {
      if (
        !p ||
        !known.has(p.action) ||
        !text(p.resourceType) ||
        !scopeValid(p.scope) ||
        !sameScope(role.scope, p.scope) ||
        (p.resourceId !== undefined && !text(p.resourceId))
      )
        fail();
      const pk = permissionKey(p);
      if (permissions.has(pk)) fail();
      permissions.add(pk);
    }
    if (new Set(role.inherits ?? []).size !== (role.inherits ?? []).length) fail();
  }
  const visited = new Set<string>();
  const visiting = new Set<string>();
  const visit = (k: string): void => {
    if (visiting.has(k)) fail();
    if (visited.has(k)) return;
    const role = roles.get(k);
    if (!role) fail();
    visiting.add(k);
    for (const id of role!.inherits ?? []) {
      if (!text(id)) fail();
      visit(key(role!.scope, id));
    }
    visiting.delete(k);
    visited.add(k);
  };
  for (const k of roles.keys()) visit(k);
  const bindings = new Set<string>();
  for (const b of graph.bindings) {
    if (
      !b ||
      !text(b.id) ||
      bindings.has(b.id) ||
      !principalValid(b.principal) ||
      !scopeValid(b.scope) ||
      !text(b.roleId) ||
      !roles.has(key(b.scope, b.roleId)) ||
      (b.expiresAt !== undefined && (!Number.isSafeInteger(b.expiresAt) || b.expiresAt < 0))
    )
      fail();
    bindings.add(b.id);
  }
}

/** Call after graph validation; inheritance is confined to the exact role scope. */
export function effectiveRolePermissions<A extends string>(
  graph: RoleGraph<A>,
  roleId: string,
  scope: Scope,
): readonly Permission<A>[] {
  const pending = [roleId];
  const visited = new Set<string>();
  const permissions = new Map<string, Permission<A>>();
  while (pending.length) {
    const id = pending.pop()!;
    if (visited.has(id)) continue;
    if (visited.size >= MAX_ROLES) throw new RoleGraphError();
    visited.add(id);
    const role = graph.roles.find((r) => r.id === id && sameScope(r.scope, scope));
    if (!role || role.permissions.length > MAX_ITEMS || (role.inherits?.length ?? 0) > MAX_ROLES) {
      throw new RoleGraphError();
    }
    for (const p of role.permissions) permissions.set(permissionKey(p), p);
    pending.push(...(role.inherits ?? []));
  }
  return [...permissions.values()];
}

export function effectivePermissions<A extends string>(
  graph: RoleGraph<A>,
  principal: Principal,
  scope: Scope,
  now: number,
): readonly Permission<A>[] {
  if (!Number.isFinite(now)) return [];
  const permissions = new Map<string, Permission<A>>();
  for (const b of graph.bindings) {
    if (
      samePrincipal(b.principal, principal) &&
      sameScope(b.scope, scope) &&
      (b.expiresAt === undefined || b.expiresAt > now)
    ) {
      for (const p of effectiveRolePermissions(graph, b.roleId, scope)) {
        permissions.set(permissionKey(p), p);
      }
    }
  }
  return [...permissions.values()];
}

export function immutableRoleSnapshot<A extends string>(
  value: RoleSnapshot<A>,
  actions: readonly A[],
): RoleSnapshot<A> {
  try {
    const copy = snapshot(value, { maxNodes: 4_000_000, maxDepth: 32 });
    if (!text(copy.revision)) throw new RoleGraphError();
    validateRoleGraph(copy.graph, actions);
    return copy;
  } catch {
    throw new RoleGraphError();
  }
}

export function memoryRoleStore<A extends string>(
  graph: RoleGraph<A>,
  actions: readonly A[],
): RoleStore<A> {
  const known = [...actions];
  let current = immutableRoleSnapshot({ revision: crypto.randomUUID(), graph }, known);
  return {
    read: () => current,
    compareAndSwap(expected, next, evaluation) {
      if (evaluation.signal.aborted || expected !== current.revision) return false;
      const candidate = immutableRoleSnapshot(next, known);
      if (candidate.revision === current.revision) throw new RoleGraphError();
      current = candidate;
      return true;
    },
  };
}

export type RbacOptions<A extends string> = { readonly actions: readonly A[] } & (
  | { readonly store: RoleStore<A>; readonly graph?: never }
  | { readonly graph: RoleGraph<A>; readonly store?: never }
);

export function rbacPredicate<A extends string, R extends Resource = Resource, C = Attributes>(
  options: RbacOptions<A>,
): Predicate<A, R, C> {
  const actions = [...options.actions];
  validateRoleGraph({ roles: [], bindings: [] }, actions);
  const store = options.store ?? memoryRoleStore(options.graph!, actions);
  return async (request, evaluation) => {
    if (
      !request.principal ||
      !actions.includes(request.action) ||
      evaluation.signal.aborted ||
      !Number.isFinite(evaluation.now)
    )
      return false;
    try {
      const current = immutableRoleSnapshot(await store.read(evaluation), actions);
      if (evaluation.signal.aborted) return false;
      return effectivePermissions(
        current.graph,
        request.principal,
        request.resource.scope,
        evaluation.now,
      ).some((p) =>
        permissionCovers(p, {
          action: request.action,
          resourceType: request.resource.type,
          resourceId: request.resource.id,
          scope: request.resource.scope,
        }),
      );
    } catch {
      throw new RoleGraphError();
    }
  };
}

export function rbacPolicy<A extends string, R extends Resource = Resource, C = Attributes>(
  options: RbacOptions<A>,
): Policy<A, R, C> {
  const test = rbacPredicate<A, R, C>(options);
  return {
    evaluate: async (request, evaluation) =>
      (await test(request, evaluation)) ? "allow" : "abstain",
  };
}
