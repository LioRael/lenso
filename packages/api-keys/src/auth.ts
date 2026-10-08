import { defineSource, type ActorKind } from "@lenso/auth";
import { namespacedSubjectId, sameKeySubject, type ApiKeys, type KeySubject } from "./index";

/** One explicit permission profile per source. Auth re-runs this verification on enforce. */
export function apiKeySource<C, R>(options: {
  readonly keys: Pick<ApiKeys<C, R>, "verify">;
  readonly requiredScopes: readonly string[];
  readonly realmId?: string;
  readonly kind?: ActorKind;
  /** Only map to another source's identity after an authoritative namespace-aware lookup. */
  readonly subjectId?: (subject: KeySubject) => string | Promise<string>;
}) {
  if (
    !Array.isArray(options.requiredScopes) ||
    !options.requiredScopes.length ||
    options.requiredScopes.length > 128 ||
    options.requiredScopes.some(
      (scope) => typeof scope !== "string" || !scope.trim() || scope.length > 256,
    )
  )
    throw new Error("API key source requires an explicit nonempty scope profile");
  const scopes = Object.freeze([...options.requiredScopes]);
  const mapSubject = options.subjectId ?? namespacedSubjectId;
  const realmId = options.realmId;
  const kind = options.kind ?? "service";
  return defineSource({
    realmId,
    async verify(evidence: string | null, context) {
      context.signal.throwIfAborted();
      if (evidence === null) return { status: "absent" };
      const key = await options.keys.verify(evidence);
      if (!key || scopes.some((scope) => !key.scopes.includes(scope)))
        return { status: "rejected" };
      const subjectId = await mapSubject(key.subject);
      const current = await options.keys.verify(evidence);
      context.signal.throwIfAborted();
      if (
        !current ||
        !sameKeySubject(key.subject, current.subject) ||
        scopes.some((scope) => !current.scopes.includes(scope))
      )
        return { status: "rejected" };
      return { status: "verified", subjectId, kind };
    },
  });
}
