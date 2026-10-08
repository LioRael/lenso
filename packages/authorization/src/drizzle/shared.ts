import type { Evaluation, RoleSnapshot, RoleStore } from "../types";
import { immutableRoleSnapshot, RoleGraphError } from "../rbac";

export function decodeSnapshot<A extends string>(
  revision: unknown,
  graph: unknown,
  actions: readonly A[],
): RoleSnapshot<A> {
  try {
    const value = typeof graph === "string" ? JSON.parse(graph) : graph;
    return immutableRoleSnapshot({ revision, graph: value } as RoleSnapshot<A>, actions);
  } catch {
    throw new RoleGraphError();
  }
}

export function createStore<A extends string>(
  namespace: string,
  actions: readonly A[],
  readRow: (evaluation: Evaluation) => Promise<{ revision: unknown; graph: unknown } | undefined>,
  insert: (snapshot: RoleSnapshot<A>) => Promise<void>,
  update: (expectedRevision: string, snapshot: RoleSnapshot<A>) => Promise<boolean>,
): RoleStore<A> & { initialize(snapshot: RoleSnapshot<A>): Promise<void> } {
  if (typeof namespace !== "string" || !namespace.length || namespace.length > 256)
    throw new RoleGraphError();
  const known = Object.freeze([...actions]);
  immutableRoleSnapshot({ revision: "validation", graph: { roles: [], bindings: [] } }, known);
  return {
    async read(evaluation) {
      try {
        evaluation.signal.throwIfAborted();
        const row = await readRow(evaluation);
        evaluation.signal.throwIfAborted();
        if (!row) throw new RoleGraphError();
        return decodeSnapshot(row.revision, row.graph, known);
      } catch {
        throw new RoleGraphError();
      }
    },
    async compareAndSwap(expectedRevision, next, evaluation) {
      try {
        evaluation.signal.throwIfAborted();
        if (
          typeof expectedRevision !== "string" ||
          !expectedRevision.length ||
          expectedRevision.length > 256 ||
          next.revision === expectedRevision
        )
          throw new RoleGraphError();
        const candidate = immutableRoleSnapshot(next, known);
        const changed = await update(expectedRevision, candidate);
        // Cancellation after commit means uncertain caller outcome, not rollback.
        evaluation.signal.throwIfAborted();
        return changed;
      } catch {
        throw new RoleGraphError();
      }
    },
    async initialize(value) {
      try {
        await insert(immutableRoleSnapshot(value, known));
      } catch {
        throw new RoleGraphError();
      }
    },
  };
}
