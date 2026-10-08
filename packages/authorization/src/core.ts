import { AuthorizationError, AuthorizationConfigurationError } from "./errors";
import { sameScope, matchesPermission } from "./conditions";
import { snapshot } from "./snapshot";
import type {
  Attributes,
  Authorization,
  AuthorizationOptions,
  Condition,
  Decision,
  Evaluation,
  Explanation,
  ReasonCode,
  Request,
  Resource,
  Rule,
} from "./types";

function identifier(value: unknown): value is string {
  return typeof value === "string" && value.length > 0 && value.length <= 256;
}

function validResource(resource: Resource): boolean {
  return (
    !!resource &&
    identifier(resource.type) &&
    identifier(resource.id) &&
    !!resource.scope &&
    identifier(resource.scope.type) &&
    identifier(resource.scope.id)
  );
}

function compileCondition<A extends string, R extends Resource, C>(
  condition: Condition<A, R, C>,
  stack = new Set<object>(),
  depth = 0,
  budget = { nodes: 0 },
): Condition<A, R, C> {
  if (!condition || stack.has(condition) || depth > 32 || ++budget.nodes > 1024)
    throw new AuthorizationConfigurationError();
  stack.add(condition);
  let result: Condition<A, R, C>;
  switch (condition.kind) {
    case "all":
    case "any":
      if (!Array.isArray(condition.conditions) || condition.conditions.length === 0)
        throw new AuthorizationConfigurationError();
      result = Object.freeze({
        kind: condition.kind,
        conditions: Object.freeze(
          condition.conditions.map((child) => compileCondition(child, stack, depth + 1, budget)),
        ),
      });
      break;
    case "predicate":
      if (typeof condition.test !== "function") throw new AuthorizationConfigurationError();
      result = Object.freeze({ kind: "predicate", test: condition.test });
      break;
    case "attribute": {
      const primitive = (item: unknown) =>
        item === null ||
        typeof item === "string" ||
        typeof item === "boolean" ||
        (typeof item === "number" && Number.isFinite(item));
      if (
        !["principal", "resource", "context"].includes(condition.source) ||
        !identifier(condition.key) ||
        (condition.operator === "equals"
          ? !primitive(condition.value)
          : condition.operator !== "in" ||
            !Array.isArray(condition.value) ||
            condition.value.length === 0 ||
            !condition.value.every(primitive))
      )
        throw new AuthorizationConfigurationError();
      result = snapshot(condition);
      break;
    }
    case "relation":
      if (!identifier(condition.relation) || (condition.target && !validResource(condition.target)))
        throw new AuthorizationConfigurationError();
      result = snapshot(condition);
      break;
    default:
      throw new AuthorizationConfigurationError();
  }
  stack.delete(condition);
  return result;
}

export function createAuthorization<
  A extends string = string,
  R extends Resource = Resource,
  C = Attributes,
>(options: AuthorizationOptions<A, R, C>): Authorization<A, R, C> {
  const actions = new Set(options.actions);
  const timeoutMs = options.timeoutMs ?? 1000;
  if (
    !actions.size ||
    actions.size !== options.actions.length ||
    [...actions].some((action) => !identifier(action)) ||
    !Number.isSafeInteger(timeoutMs) ||
    timeoutMs < 1 ||
    timeoutMs > 60_000
  )
    throw new AuthorizationConfigurationError();
  const ids = new Set<string>();
  const rules: readonly Rule<A, R, C>[] = Object.freeze(
    (options.rules ?? []).map((rule) => {
      if (
        !identifier(rule.id) ||
        ids.has(rule.id) ||
        !["allow", "deny"].includes(rule.effect) ||
        !identifier(rule.resourceType) ||
        !rule.actions.length ||
        rule.actions.some((action) => !actions.has(action)) ||
        (rule.scope && (!identifier(rule.scope.type) || !identifier(rule.scope.id))) ||
        (rule.resourceId !== undefined && !identifier(rule.resourceId))
      )
        throw new AuthorizationConfigurationError();
      ids.add(rule.id);
      return Object.freeze({
        ...rule,
        actions: Object.freeze([...rule.actions]),
        ...(rule.scope ? { scope: snapshot(rule.scope) } : {}),
        when: compileCondition(rule.when),
      });
    }),
  );
  const policies = Object.freeze(
    (options.policies ?? []).map((policy) => {
      if (typeof policy.evaluate !== "function") throw new AuthorizationConfigurationError();
      return policy.evaluate.bind(policy);
    }),
  );
  const boundaries = Object.freeze([...(options.boundaries ?? [])]);
  if (boundaries.some((boundary) => typeof boundary !== "function"))
    throw new AuthorizationConfigurationError();
  const identity = snapshot(options.identity ?? {});
  if (
    [identity.required, identity.credentialRequired].some(
      (value) => value !== undefined && typeof value !== "boolean",
    )
  )
    throw new AuthorizationConfigurationError();
  for (const values of [identity.realms, identity.audiences]) {
    if (
      values !== undefined &&
      (!Array.isArray(values) || !values.length || values.some((value) => !identifier(value)))
    )
      throw new AuthorizationConfigurationError();
  }
  const resolveResource = options.resolveResource;
  const resolveRelation = options.relations?.check.bind(options.relations);
  const resolveAttributes = options.attributes?.resolve.bind(options.attributes);
  const clock = options.clock ?? Date.now;
  const observe = options.observe;
  const explainGate = options.explain ? snapshot(options.explain) : undefined;
  if (explainGate && (!actions.has(explainGate.action) || !validResource(explainGate.resource)))
    throw new AuthorizationConfigurationError();

  async function conditionMatches(
    condition: Condition<A, R, C>,
    request: Request<A, R, C>,
    evaluation: Evaluation,
  ): Promise<boolean> {
    evaluation.signal.throwIfAborted();
    switch (condition.kind) {
      case "all":
      case "any": {
        const results: boolean[] = [];
        // Evaluate every branch: an error in a deny-relevant branch must not be masked by OR.
        for (const child of condition.conditions)
          results.push(await conditionMatches(child, request, evaluation));
        return condition.kind === "all" ? results.every(Boolean) : results.some(Boolean);
      }
      case "predicate": {
        const result = await condition.test(request, evaluation);
        if (typeof result !== "boolean") throw new Error("Invalid predicate outcome");
        return result;
      }
      case "attribute": {
        const facts =
          condition.source === "principal"
            ? request.principal?.attributes
            : condition.source === "resource"
              ? request.resource.attributes
              : request.context;
        if (!facts || typeof facts !== "object" || !Object.hasOwn(facts, condition.key))
          return false;
        const value = (facts as Record<string, unknown>)[condition.key];
        return condition.operator === "equals"
          ? value === condition.value
          : (condition.value as readonly unknown[]).includes(value);
      }
      case "relation": {
        if (!request.principal) return false;
        if (!resolveRelation) throw new Error("Missing resolver");
        const result = await resolveRelation(
          request.principal,
          condition.relation,
          condition.target ?? request.resource,
          evaluation,
        );
        if (typeof result !== "boolean") throw new Error("Invalid relation outcome");
        return result;
      }
    }
  }

  async function evaluate(input: Request<A, R, C>, signal?: AbortSignal): Promise<Explanation> {
    const controller = new AbortController();
    const combined = signal ? AbortSignal.any([signal, controller.signal]) : controller.signal;
    const started = performance.now();
    let timedOut = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const paths: string[] = [];
    const decision = (code: ReasonCode): Explanation =>
      Object.freeze({
        decision: Object.freeze({ effect: code === "ALLOWED" ? "allow" : "deny", code }),
        paths: Object.freeze([...paths]),
      });
    const stopped = () =>
      timedOut || performance.now() - started >= timeoutMs
        ? "TIMEOUT"
        : signal?.aborted
          ? "CANCELLED"
          : "EVALUATION_FAILED";
    try {
      const request = snapshot(input);
      const now = clock();
      if (
        !Number.isSafeInteger(now) ||
        now < 0 ||
        !validResource(request.resource) ||
        (request.principal !== null &&
          (!request.principal ||
            !identifier(request.principal.realmId) ||
            !identifier(request.principal.subjectId) ||
            !identifier(request.principal.kind)))
      )
        return decision("EVALUATION_FAILED");
      if (!actions.has(request.action)) return decision("UNKNOWN_ACTION");
      if (signal?.aborted) return decision("CANCELLED");
      const interrupted = new Promise<Explanation>((resolve) => {
        timer = setTimeout(() => {
          timedOut = true;
          controller.abort();
        }, timeoutMs);
        combined.addEventListener("abort", () => resolve(decision(stopped())), { once: true });
      });
      const work = async (): Promise<Explanation> => {
        const evaluation = Object.freeze({ signal: combined, now });
        if (
          (identity.required === true && !request.principal) ||
          (identity.realms &&
            (!request.principal || !identity.realms.includes(request.principal.realmId))) ||
          (identity.audiences &&
            (!request.audience || !identity.audiences.includes(request.audience))) ||
          (identity.credentialRequired === true && !request.credential)
        )
          return decision("BOUNDARY_DENIED");
        const resolved = resolveResource
          ? snapshot(await resolveResource(request.resource, evaluation))
          : request.resource;
        if (
          !validResource(resolved) ||
          resolved.type !== request.resource.type ||
          resolved.id !== request.resource.id
        )
          return decision("BOUNDARY_DENIED");
        let facts: Request<A, R, C> = Object.freeze({ ...request, resource: resolved });
        if (facts.credential) {
          const credential = facts.credential;
          if (
            !Array.isArray(credential.permissions) ||
            credential.permissions.some(
              (permission) =>
                !permission ||
                !actions.has(permission.action) ||
                !identifier(permission.resourceType) ||
                !permission.scope ||
                !identifier(permission.scope.type) ||
                !identifier(permission.scope.id) ||
                (permission.resourceId !== undefined && !identifier(permission.resourceId)),
            ) ||
            (credential.expiresAt !== undefined &&
              (!Number.isSafeInteger(credential.expiresAt) || credential.expiresAt <= now)) ||
            !credential.permissions.some((permission) => matchesPermission(permission, facts))
          )
            return decision("BOUNDARY_DENIED");
        }
        if (resolveAttributes) {
          const attributes = snapshot(await resolveAttributes(facts, evaluation));
          const validAttributes = (value: unknown) =>
            value === undefined ||
            (value !== null && typeof value === "object" && !Array.isArray(value));
          if (
            !validAttributes(attributes.principal) ||
            !validAttributes(attributes.resource) ||
            (attributes.principal !== undefined && facts.principal === null)
          )
            throw new Error("Invalid resolved attributes");
          facts = snapshot({
            ...facts,
            principal:
              facts.principal && attributes.principal !== undefined
                ? { ...facts.principal, attributes: attributes.principal }
                : facts.principal,
            resource:
              attributes.resource !== undefined
                ? { ...facts.resource, attributes: attributes.resource }
                : facts.resource,
            context: attributes.context === undefined ? facts.context : attributes.context,
          });
        }
        for (const boundary of boundaries) {
          if ((await boundary(facts, evaluation)) !== true) return decision("BOUNDARY_DENIED");
        }
        let allowed = false;
        let denied = false;
        for (let index = 0; index < rules.length; index++) {
          const rule = rules[index]!;
          if (
            !rule.actions.includes(facts.action) ||
            rule.resourceType !== facts.resource.type ||
            (rule.scope && !sameScope(rule.scope, facts.resource.scope)) ||
            (rule.resourceId !== undefined && rule.resourceId !== facts.resource.id)
          )
            continue;
          if (await conditionMatches(rule.when, facts, evaluation)) {
            paths.push(`rules/${index}/${rule.effect}`);
            if (rule.effect === "deny") denied = true;
            else allowed = true;
          }
        }
        for (let index = 0; index < policies.length; index++) {
          combined.throwIfAborted();
          const effect = await policies[index]!(facts, evaluation);
          if (!["allow", "deny", "abstain"].includes(effect))
            throw new Error("Invalid policy outcome");
          if (effect !== "abstain") paths.push(`policies/${index}/${effect}`);
          if (effect === "deny") denied = true;
          if (effect === "allow") allowed = true;
        }
        const result = decision(denied ? "EXPLICIT_DENY" : allowed ? "ALLOWED" : "DEFAULT_DENY");
        if (observe) await observe(result.decision);
        combined.throwIfAborted();
        if (performance.now() - started >= timeoutMs) return decision("TIMEOUT");
        return result;
      };
      return await Promise.race([work().catch(() => decision(stopped())), interrupted]);
    } catch {
      return decision(stopped());
    } finally {
      if (timer !== undefined) clearTimeout(timer);
      controller.abort();
    }
  }

  const check = async (
    request: Request<A, R, C>,
    opts?: { signal?: AbortSignal },
  ): Promise<Decision> => (await evaluate(request, opts?.signal)).decision;
  return Object.freeze<Authorization<A, R, C>>({
    check,
    async can(request, opts) {
      return (await check(request, opts)).effect === "allow";
    },
    async enforce(request, opts) {
      if ((await check(request, opts)).effect !== "allow") throw new AuthorizationError();
    },
    async explain(manager, target, opts) {
      let managerFacts: Request<A, R, C>;
      let targetFacts: Request<A, R, C>;
      try {
        managerFacts = snapshot(manager);
        targetFacts = snapshot(target);
      } catch {
        throw new AuthorizationError();
      }
      if (
        !explainGate ||
        managerFacts.action !== explainGate.action ||
        managerFacts.resource.type !== explainGate.resource.type ||
        managerFacts.resource.id !== explainGate.resource.id ||
        !sameScope(managerFacts.resource.scope, explainGate.resource.scope) ||
        (await check(managerFacts, opts)).effect !== "allow"
      )
        throw new AuthorizationError();
      return evaluate(targetFacts, opts?.signal);
    },
  });
}
