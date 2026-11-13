import { ActionError } from "./error.js";
import type { Context } from "./action.js";

export type Policy<I, R> = (context: Context<R>, input: I) => void | Promise<void>;

/** Checks all policies in order, stopping before later checks after a denial or failure. */
export function allOf<I, R>(...policies: readonly Policy<I, R>[]): Policy<I, R> {
  return async (context, input) => {
    for (const policy of policies) await policy(context, input);
  };
}

/** Alternatives handle access denials only; dependency and implementation failures propagate. */
export function anyOf<I, R>(...policies: readonly Policy<I, R>[]): Policy<I, R> {
  return async (context, input) => {
    let denial: ActionError | undefined;
    for (const policy of policies) {
      try { await policy(context, input); return; }
      catch (error) {
        if (!(error instanceof ActionError) || (error.code !== "forbidden" && error.code !== "unauthenticated")) throw error;
        if (!denial || error.code === "forbidden") denial = error;
      }
    }
    throw denial ?? new ActionError("forbidden", "Permission denied");
  };
}

/** Established by trusted authentication code, never copied from request JSON. */
export interface Principal {
  readonly subject: string;
  readonly scopes: readonly string[];
}

export function requirePrincipal(principal: Principal | undefined): Principal {
  if (!principal) throw new ActionError("unauthenticated", "Authentication required");
  return principal;
}

export function requireScope(principal: Principal | undefined, scope: string): void {
  if (!requirePrincipal(principal).scopes.includes(scope)) {
    throw new ActionError("forbidden", "Permission denied");
  }
}

export function requireOwner(principal: Principal | undefined, owner: string): void {
  if (requirePrincipal(principal).subject !== owner) {
    throw new ActionError("forbidden", "Permission denied");
  }
}
