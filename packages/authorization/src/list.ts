import { AuthorizationError } from "./errors";
import { snapshot } from "./snapshot";
import type { Attributes, Authorization, Request, Resource } from "./types";

/** Evaluate a complete bounded candidate set, then paginate; never reuse backend totals. */
export async function authorizeList<A extends string, R extends Resource, C = Attributes>(
  authorization: Authorization<A, R, C>,
  candidates: readonly R[],
  request: Omit<Request<A, R, C>, "resource">,
  options: { maxCandidates: number; offset?: number; limit?: number; signal?: AbortSignal },
): Promise<{ readonly items: readonly R[]; readonly total: number }> {
  const { maxCandidates, offset = 0, limit = maxCandidates, signal } = options;
  if (
    !Number.isSafeInteger(maxCandidates) ||
    maxCandidates < 1 ||
    maxCandidates > 1000 ||
    candidates.length > maxCandidates ||
    !Number.isSafeInteger(offset) ||
    offset < 0 ||
    !Number.isSafeInteger(limit) ||
    limit < 0 ||
    limit > maxCandidates
  )
    throw new AuthorizationError();
  const facts = snapshot(request);
  const resources = snapshot(candidates);
  const visible: R[] = [];
  for (const resource of resources) {
    const decision = await authorization.check({ ...facts, resource }, { signal });
    if (["EVALUATION_FAILED", "TIMEOUT", "CANCELLED", "UNKNOWN_ACTION"].includes(decision.code))
      throw new AuthorizationError();
    if (decision.effect === "allow") visible.push(resource);
  }
  return Object.freeze({
    items: Object.freeze(visible.slice(offset, offset + limit)),
    total: visible.length,
  });
}
