import { definePlugin, type Plugin } from "@lenso/core/plugin";
import { defineOperation } from "@lenso/engine/operations";
import { defineManage } from "@lenso/manage";
import { z } from "zod";
import { AuthorizationError } from "./errors";
import { immutableRoleSnapshot, sameScope } from "./rbac";
import type { Awaitable, Binding, Evaluation, RoleSnapshot, RoleStore, Scope } from "./types";

export interface AuthorizationInspection<C, A extends string = string> {
  inspect(scope: Scope, caller: C, evaluation: Evaluation): Promise<RoleSnapshot<A>>;
}

/** The repository stays private; authorization is repeated after the asynchronous read. */
export function createAuthorizationInspection<C, A extends string>(options: {
  readonly store: RoleStore<A>;
  readonly actions: readonly A[];
  readonly authorize: (caller: C, scope: Scope, evaluation: Evaluation) => Awaitable<boolean>;
  readonly authorizeBinding: (
    caller: C,
    binding: Binding,
    evaluation: Evaluation,
  ) => Awaitable<boolean>;
}): AuthorizationInspection<C, A> {
  return Object.freeze({
    async inspect(scope: Scope, caller: C, evaluation: Evaluation) {
      const boundScope = Object.freeze({ type: scope.type, id: scope.id });
      const boundEvaluation = Object.freeze({ now: evaluation.now, signal: evaluation.signal });
      boundEvaluation.signal.throwIfAborted();
      if (
        !Number.isSafeInteger(boundEvaluation.now) ||
        !boundScope.type ||
        !boundScope.id ||
        (await options.authorize(caller, boundScope, boundEvaluation)) !== true
      ) {
        throw new AuthorizationError();
      }
      const current = immutableRoleSnapshot(
        await options.store.read(boundEvaluation),
        options.actions,
      );
      boundEvaluation.signal.throwIfAborted();
      const bindings: Binding[] = [];
      for (const binding of current.graph.bindings) {
        if (!sameScope(binding.scope, boundScope)) continue;
        boundEvaluation.signal.throwIfAborted();
        if ((await options.authorizeBinding(caller, binding, boundEvaluation)) === true) {
          bindings.push(binding);
        }
      }
      if ((await options.authorize(caller, boundScope, boundEvaluation)) !== true) {
        throw new AuthorizationError();
      }
      boundEvaluation.signal.throwIfAborted();
      return Object.freeze({
        revision: current.revision,
        graph: Object.freeze({
          roles: Object.freeze(
            current.graph.roles.filter((role) => sameScope(role.scope, boundScope)),
          ),
          bindings: Object.freeze(bindings),
        }),
      });
    },
  });
}

/** Explicit metadata-only exposure; role mutation requires a separate audited revision contract. */
export function createAuthorizationManage<C, A extends string>(
  inspection: Plugin<AuthorizationInspection<C, A>>,
  id: string,
) {
  const plugin = definePlugin({
    id,
    requires: [inspection],
    setup(context) {
      const service = context.get(inspection);
      return {
        inspect: (input: { scope: Scope }, invocation: { caller: C; evaluation: Evaluation }) =>
          service.inspect(input.scope, invocation.caller, invocation.evaluation),
      };
    },
  });
  const identifier = z
    .string()
    .min(1)
    .max(256)
    .refine((value) => !!value.trim());
  const operation = defineOperation({
    plugin,
    method: "inspect",
    context: true,
    input: z
      .object({
        scope: z.object({ type: identifier, id: identifier }).strict(),
      })
      .strict(),
    effect: "read",
    retry: "safe",
    cancellation: "cooperative",
    mapError: (error) =>
      error instanceof AuthorizationError
        ? { code: "denied", phase: "invoke", message: "Authorization operation denied" }
        : undefined,
    description: "Inspect authorized roles and bindings in one exact scope.",
  });
  const operations = [operation];
  return { plugin, operations, manage: defineManage({ plugin, operations }) };
}
