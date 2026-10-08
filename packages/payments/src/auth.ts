import type { Access, Actor, Policy, PolicyContext } from "@lenso/auth";
import type { PaymentAction, PaymentResource, PaymentsAuthorization } from "./contracts";

/** Preserve Auth instance/audience provenance and reverify evidence on every shared service call. */
export function paymentsAuthorization<R extends string, E, S extends string, A extends string, M>(
  access: Access<R, E, S, A, PaymentResource, M>,
  policies: Record<PaymentAction, Policy<PolicyContext<Actor<R, S, A>, PaymentResource, M>>>,
): PaymentsAuthorization<Actor<R, S, A>> {
  return async (actor, action, record) => {
    await access.enforce(actor, record, policies[action]);
  };
}
