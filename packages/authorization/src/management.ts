import type {
  Awaitable,
  Binding,
  Evaluation,
  Permission,
  Principal,
  Role,
  RoleGraph,
  RoleStore,
  Scope,
} from "./types";
import {
  effectivePermissions,
  effectiveRolePermissions,
  immutableRoleSnapshot,
  permissionCovers,
  samePrincipal,
  sameScope,
} from "./rbac";
import { snapshot } from "./snapshot";

export type ManagementAction = "use" | "grant" | "edit" | "bind" | "revoke" | "delegate";
export interface ManagementRequest {
  readonly actor: Principal;
  readonly action: ManagementAction;
  readonly scope: Scope;
  readonly targetId: string;
  readonly role?: Role;
  readonly binding?: Binding;
  readonly sourceBindingId?: string;
  readonly evaluation: Evaluation;
}
export interface GrantAuthority<A extends string = string> {
  readonly permissions: readonly Permission<A>[];
  readonly scopes: readonly Scope[];
  readonly maxExpiresAt: number;
}
export interface RoleMutationResult {
  readonly revision: string;
}
export interface RoleManagementOptions<A extends string> {
  readonly store: RoleStore<A>;
  readonly actions: readonly A[];
  /**
   * Trusted application adapter: invoke Auth/authorization enforcement here.
   * Supply a deadline signal in Evaluation; adapters must honor it.
   * Cancellation cannot undo an already committed CAS.
   */
  readonly authorize: (request: ManagementRequest) => Awaitable<boolean>;
  readonly grantAuthority: (request: ManagementRequest) => Awaitable<GrantAuthority<A> | null>;
}
export class RoleManagementError extends Error {
  readonly code: "DENIED" | "CONFLICT";
  constructor(code: "DENIED" | "CONFLICT" = "DENIED") {
    super(code === "CONFLICT" ? "Concurrent modification" : "Access denied");
    this.name = "RoleManagementError";
    this.code = code;
  }
}

function cloneFacts<T>(value: T): T {
  try {
    return snapshot(value);
  } catch {
    throw new RoleManagementError();
  }
}

function invocation(evaluation: Evaluation): Evaluation {
  return Object.freeze({ now: evaluation.now, signal: evaluation.signal });
}

export function createRoleManagement<A extends string>(options: RoleManagementOptions<A>) {
  if (typeof options.authorize !== "function" || typeof options.grantAuthority !== "function") {
    throw new RoleManagementError();
  }
  const actions = [...options.actions];
  const denied = (): never => {
    throw new RoleManagementError();
  };
  async function mutate(
    actor: Principal,
    action: ManagementAction,
    scope: Scope,
    targetId: string,
    evaluation: Evaluation,
    change: (graph: RoleGraph<A>, authority: GrantAuthority<A> | null) => RoleGraph<A>,
    proposal: {
      readonly role?: Role<A>;
      readonly binding?: Binding;
      readonly sourceBindingId?: string;
    } = {},
  ): Promise<RoleMutationResult> {
    try {
      if (
        !actor ||
        !actor.realmId ||
        !actor.subjectId ||
        !actor.kind ||
        !Number.isFinite(evaluation.now) ||
        evaluation.signal.aborted
      )
        denied();
      const request = cloneFacts({ actor, action, scope, targetId, ...proposal });
      const trustedRequest = Object.freeze({ ...request, evaluation });
      // Fence authorization read from this same graph as well as the proposed mutation.
      const current = immutableRoleSnapshot(await options.store.read(evaluation), actions);
      if (evaluation.signal.aborted) denied();
      if ((await options.authorize(trustedRequest)) !== true) denied();
      if (evaluation.signal.aborted) denied();
      const authority =
        action === "revoke" ? null : cloneFacts(await options.grantAuthority(trustedRequest));
      if (evaluation.signal.aborted) denied();
      if (
        action !== "revoke" &&
        (!authority ||
          !Number.isFinite(authority.maxExpiresAt) ||
          authority.maxExpiresAt <= evaluation.now ||
          !authority.scopes.some((s) => sameScope(s, scope)))
      )
        denied();
      const graph = change(current.graph, authority);
      const next = immutableRoleSnapshot({ revision: crypto.randomUUID(), graph }, actions);
      if (evaluation.signal.aborted) denied();
      if (!(await options.store.compareAndSwap(current.revision, next, evaluation))) {
        throw new RoleManagementError("CONFLICT");
      }
      if (evaluation.signal.aborted) denied();
      return Object.freeze({ revision: next.revision });
    } catch (error) {
      if (error instanceof RoleManagementError) throw error;
      throw new RoleManagementError();
    }
  }
  function within(permissions: readonly Permission<A>[], authority: GrantAuthority<A>): void {
    if (
      permissions.some(
        (p) =>
          !authority.scopes.some((s) => sameScope(s, p.scope)) ||
          !authority.permissions.some((c) => permissionCovers(c, p)),
      )
    )
      denied();
  }
  function expiry(binding: Binding, authority: GrantAuthority<A>, now: number): void {
    if (
      binding.expiresAt === undefined ||
      !Number.isFinite(binding.expiresAt) ||
      binding.expiresAt <= now ||
      binding.expiresAt > authority.maxExpiresAt
    )
      denied();
  }
  return {
    async createRole(actor: Principal, role: Role<A>, evaluation: Evaluation) {
      evaluation = invocation(evaluation);
      actor = cloneFacts(actor);
      const proposed = cloneFacts(role);
      return mutate(
        actor,
        "grant",
        proposed.scope,
        proposed.id,
        evaluation,
        (graph, authority) => {
          const next = { ...graph, roles: [...graph.roles, proposed] };
          immutableRoleSnapshot({ revision: "validation", graph: next }, actions);
          within(effectiveRolePermissions(next, proposed.id, proposed.scope), authority!);
          return next;
        },
        { role: proposed },
      );
    },
    async editRole(actor: Principal, role: Role<A>, evaluation: Evaluation) {
      evaluation = invocation(evaluation);
      actor = cloneFacts(actor);
      const proposed = cloneFacts(role);
      return mutate(
        actor,
        "edit",
        proposed.scope,
        proposed.id,
        evaluation,
        (graph, authority) => {
          if (!graph.roles.some((r) => r.id === proposed.id && sameScope(r.scope, proposed.scope)))
            denied();
          const next = {
            ...graph,
            roles: graph.roles.map((r) =>
              r.id === proposed.id && sameScope(r.scope, proposed.scope) ? proposed : r,
            ),
          };
          immutableRoleSnapshot({ revision: "validation", graph: next }, actions);
          // Check descendants too: editing a base role changes their effective grants.
          for (const r of next.roles.filter((candidate) =>
            sameScope(candidate.scope, proposed.scope),
          )) {
            const after = effectiveRolePermissions(next, r.id, r.scope);
            const before = effectiveRolePermissions(graph, r.id, r.scope);
            const changed =
              after.some((p) => !before.some((c) => permissionCovers(c, p))) ||
              before.some((p) => !after.some((c) => permissionCovers(c, p)));
            if (r.id === proposed.id || changed) {
              within(after, authority!);
              for (const b of graph.bindings.filter(
                (candidate) =>
                  candidate.roleId === r.id &&
                  sameScope(candidate.scope, r.scope) &&
                  (candidate.expiresAt === undefined || candidate.expiresAt > evaluation.now),
              )) {
                expiry(b, authority!, evaluation.now);
              }
            }
          }
          const before = effectivePermissions(graph, actor, proposed.scope, evaluation.now);
          const after = effectivePermissions(next, actor, proposed.scope, evaluation.now);
          if (after.some((p) => !before.some((c) => permissionCovers(c, p)))) denied();
          return next;
        },
        { role: proposed },
      );
    },
    async bindRole(actor: Principal, binding: Binding, evaluation: Evaluation) {
      evaluation = invocation(evaluation);
      actor = cloneFacts(actor);
      const proposed = cloneFacts(binding);
      return mutate(
        actor,
        "bind",
        proposed.scope,
        proposed.id,
        evaluation,
        (graph, authority) => {
          expiry(proposed, authority!, evaluation.now);
          within(effectiveRolePermissions(graph, proposed.roleId, proposed.scope), authority!);
          return { ...graph, bindings: [...graph.bindings, proposed] };
        },
        { binding: proposed },
      );
    },
    async revokeBinding(actor: Principal, scope: Scope, bindingId: string, evaluation: Evaluation) {
      evaluation = invocation(evaluation);
      actor = cloneFacts(actor);
      scope = cloneFacts(scope);
      return mutate(actor, "revoke", scope, bindingId, evaluation, (graph) => {
        if (!graph.bindings.some((b) => b.id === bindingId && sameScope(b.scope, scope))) denied();
        return { ...graph, bindings: graph.bindings.filter((b) => b.id !== bindingId) };
      });
    },
    /** Issues an independent binding; revoking the source does not cascade to it. */
    async delegateRole(
      actor: Principal,
      sourceBindingId: string,
      binding: Binding,
      evaluation: Evaluation,
    ) {
      evaluation = invocation(evaluation);
      actor = cloneFacts(actor);
      const proposed = cloneFacts(binding);
      return mutate(
        actor,
        "delegate",
        proposed.scope,
        proposed.id,
        evaluation,
        (graph, authority) => {
          const source = graph.bindings.find(
            (b) =>
              b.id === sourceBindingId &&
              samePrincipal(b.principal, actor) &&
              sameScope(b.scope, proposed.scope) &&
              (b.expiresAt === undefined || b.expiresAt > evaluation.now),
          );
          if (!source) denied();
          expiry(proposed, authority!, evaluation.now);
          if (source!.expiresAt !== undefined && proposed.expiresAt! > source!.expiresAt) denied();
          const permissions = effectiveRolePermissions(graph, proposed.roleId, proposed.scope);
          within(permissions, authority!);
          const sourcePermissions = effectiveRolePermissions(graph, source!.roleId, source!.scope);
          if (permissions.some((p) => !sourcePermissions.some((c) => permissionCovers(c, p))))
            denied();
          return { ...graph, bindings: [...graph.bindings, proposed] };
        },
        { binding: proposed, sourceBindingId },
      );
    },
  };
}
