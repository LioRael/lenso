import { AuthConfigurationError, AuthError } from "./errors";
import {
  allRequirements,
  checkCapabilities,
  checkSession,
  type SessionRequirements,
} from "./requirements";
import type {
  ActorKind,
  AuthenticationResult,
  AuthSource,
  SessionEvidence,
  SourceCapabilities,
  VerificationContext,
} from "./source";
import { snapshotVerification } from "./source";

declare const actorBrand: unique symbol;
declare const audienceBrand: unique symbol;

export interface SubjectRef<R extends string = string, S extends string = string> {
  readonly realmId: R;
  readonly subjectId: S;
}

export interface Actor<
  R extends string = string,
  S extends string = string,
  A extends string = string,
> extends SubjectRef<R, S> {
  readonly audience: A;
  readonly kind: ActorKind;
  readonly [actorBrand]: readonly [R, S, A];
}

export interface Audience<A extends string = string> {
  readonly id: A;
  readonly [audienceBrand]: A;
}

export interface Realm<R extends string, E, S extends string> {
  readonly id: R;
  readonly source: AuthSource<E, S>;
}

function identifier(value: string): void {
  if (typeof value !== "string" || !value.trim() || value.length > 256 || /\s/.test(value)) {
    throw new AuthConfigurationError("Auth identifiers must be nonempty and contain no whitespace");
  }
}

export function audience<const A extends string>(id: A): Audience<A> {
  identifier(id);
  return Object.freeze({ id }) as Audience<A>;
}

export function realm<const R extends string, E, S extends string>(
  id: R,
  source: AuthSource<E, S>,
): Realm<R, E, S> {
  identifier(id);
  if (source.realmId !== undefined && source.realmId !== id) {
    throw new AuthConfigurationError("Source belongs to a different realm");
  }
  return Object.freeze({ id, source });
}

export interface AuthenticationOptions {
  readonly signal?: AbortSignal;
}

export interface PolicyContext<P, T, M> {
  readonly principal: P;
  readonly resource: T;
  readonly membership: M;
  readonly signal: AbortSignal;
}

export type Policy<C> = (context: C) => boolean | Promise<boolean>;

export type MembershipReader<R extends string, S extends string, T, M> = (
  subject: SubjectRef<R, S>,
  resource: T,
  context: VerificationContext,
) => M | null | Promise<M | null>;

export interface Access<
  R extends string,
  E,
  S extends string,
  A extends string,
  T = unknown,
  M = undefined,
> {
  readonly realmId: R;
  readonly audience: Audience<A>;
  optional(evidence: E, options?: AuthenticationOptions): Promise<Actor<R, S, A> | null>;
  required(evidence: E, options?: AuthenticationOptions): Promise<Actor<R, S, A>>;
  requireSession(...requirements: readonly SessionRequirements[]): Access<R, E, S, A, T, M>;
  memberships<U, G>(reader: MembershipReader<R, S, U, G>): Access<R, E, S, A, U, G>;
  enforce<U extends T>(
    actor: Actor<R, S, A> | null,
    resource: U,
    policy: Policy<PolicyContext<Actor<R, S, A>, U, M>>,
    options?: AuthenticationOptions,
  ): Promise<Actor<R, S, A>>;
}

export type ActorOf<T> = T extends {
  required: (...args: never[]) => Promise<infer P>;
}
  ? P
  : never;

export interface Auth<R extends string, E, S extends string> {
  for<const A extends string>(target: Audience<A>): Access<R, E, S, A>;
  close(): Promise<void>;
}

export function allPolicies<C>(...policies: readonly Policy<C>[]): Policy<C> {
  if (!policies.length) throw new AuthConfigurationError("At least one policy is required");
  return async (context) => {
    for (const policy of policies) {
      if ((await policy(context)) !== true) return false;
    }
    return true;
  };
}

interface Proof<E> {
  readonly evidence: E;
  readonly signal: AbortSignal;
}

function validTime(value: unknown, now?: number): value is number {
  return (
    typeof value === "number" &&
    Number.isSafeInteger(value) &&
    value >= 0 &&
    (now === undefined || value <= now)
  );
}

function validateSession(session: SessionEvidence | undefined, now: number): void {
  if (session === undefined) return;
  if (
    !session ||
    !validTime(session.expiresAt) ||
    session.expiresAt <= now ||
    (session.sessionCreatedAt !== undefined && !validTime(session.sessionCreatedAt, now)) ||
    (session.authenticatedAt !== undefined && !validTime(session.authenticatedAt, now)) ||
    (session.authoritative !== undefined && typeof session.authoritative !== "boolean") ||
    (session.assurance !== undefined &&
      (!Array.isArray(session.assurance) ||
        session.assurance.some(
          (value) => typeof value !== "string" || !value.trim() || value.length > 128,
        )))
  ) {
    throw new AuthError("UNAUTHORIZED");
  }
}

class Runtime<R extends string, E, S extends string> {
  private readonly controller = new AbortController();
  private readonly tasks = new Set<Promise<unknown>>();
  private readonly proofs = new WeakMap<object, Proof<E>>();
  private readonly verify: AuthSource<E, S>["verify"];
  readonly capabilities: SourceCapabilities;
  private stopping?: Promise<void>;
  private stopped = false;

  constructor(
    readonly definition: Realm<R, E, S>,
    private readonly clock: () => number,
  ) {
    this.verify = definition.source.verify.bind(definition.source);
    this.capabilities = Object.freeze({
      ...definition.source.capabilities,
      ...(definition.source.capabilities?.assurance === undefined
        ? {}
        : { assurance: Object.freeze([...definition.source.capabilities.assurance]) }),
    });
  }

  run<T>(signals: readonly (AbortSignal | undefined)[], work: (signal: AbortSignal) => Promise<T>) {
    if (this.stopped) return Promise.reject<T>(new AuthError("SERVICE_UNAVAILABLE"));
    const signal = AbortSignal.any([
      this.controller.signal,
      ...signals.filter((item): item is AbortSignal => item !== undefined),
    ]);
    const task = Promise.resolve().then(async () => {
      if (this.stopped) throw new AuthError("SERVICE_UNAVAILABLE");
      signal.throwIfAborted();
      try {
        const result = await work(signal);
        signal.throwIfAborted();
        return result;
      } catch (error) {
        signal.throwIfAborted();
        if (error instanceof AuthError) throw new AuthError(error.code);
        throw new AuthError("SERVICE_UNAVAILABLE");
      }
    });
    this.tasks.add(task);
    void task.then(
      () => this.tasks.delete(task),
      () => this.tasks.delete(task),
    );
    return task;
  }

  async authenticate(
    evidence: E,
    requirements: SessionRequirements,
    signal: AbortSignal,
  ): Promise<Extract<AuthenticationResult<S>, { status: "verified" }> | null> {
    const outcome = await this.verify(evidence, {
      signal,
      authoritative: requirements.authoritative === true,
    });
    signal.throwIfAborted();
    if (outcome?.status === "absent") return null;
    if (outcome?.status !== "verified") throw new AuthError("UNAUTHORIZED");
    const result = snapshotVerification(outcome);
    if (
      typeof result.subjectId !== "string" ||
      !result.subjectId.trim() ||
      result.subjectId.length > 512 ||
      (result.kind !== undefined && !["user", "guest", "service"].includes(result.kind))
    ) {
      throw new AuthError("UNAUTHORIZED");
    }
    const now = this.clock();
    if (!validTime(now)) throw new AuthError("SERVICE_UNAVAILABLE");
    validateSession(result.session, now);
    checkSession(requirements, result.session, now);
    return result;
  }

  mint<A extends string>(
    target: Audience<A>,
    verified: Extract<AuthenticationResult<S>, { status: "verified" }>,
    evidence: E,
    signal: AbortSignal,
  ): Actor<R, S, A> {
    const actor = Object.freeze({
      realmId: this.definition.id,
      subjectId: verified.subjectId,
      audience: target.id,
      kind: verified.kind ?? "user",
    }) as Actor<R, S, A>;
    this.proofs.set(actor, { evidence, signal });
    return actor;
  }

  checkEvidence(session: SessionEvidence | undefined, requirements: SessionRequirements): void {
    const now = this.clock();
    if (!validTime(now)) throw new AuthError("SERVICE_UNAVAILABLE");
    validateSession(session, now);
    checkSession(requirements, session, now);
  }

  proof<A extends string>(actor: Actor<R, S, A> | null, target: Audience<A>): Proof<E> {
    if (!actor || typeof actor !== "object") throw new AuthError("UNAUTHORIZED");
    const proof = this.proofs.get(actor);
    if (!proof || actor.realmId !== this.definition.id || actor.audience !== target.id) {
      throw new AuthError("UNAUTHORIZED");
    }
    return proof;
  }

  close(): Promise<void> {
    if (!this.stopping) {
      this.stopped = true;
      this.stopping = Promise.resolve().then(async () => {
        this.controller.abort(new AuthError("SERVICE_UNAVAILABLE"));
        await Promise.allSettled(this.tasks);
      });
    }
    return this.stopping;
  }
}

function access<R extends string, E, S extends string, A extends string, T, M>(
  runtime: Runtime<R, E, S>,
  target: Audience<A>,
  requirements: SessionRequirements,
  reader?: MembershipReader<R, S, T, M>,
): Access<R, E, S, A, T, M> {
  checkCapabilities(requirements, runtime.capabilities);

  const optional = (evidence: E, options: AuthenticationOptions = {}) =>
    runtime.run([options.signal], async (signal) => {
      const verified = await runtime.authenticate(evidence, requirements, signal);
      return verified === null ? null : runtime.mint(target, verified, evidence, signal);
    });

  return Object.freeze({
    realmId: runtime.definition.id,
    audience: target,
    optional,
    async required(evidence: E, options?: AuthenticationOptions) {
      const actor = await optional(evidence, options);
      if (!actor) throw new AuthError("UNAUTHORIZED");
      return actor;
    },
    requireSession(...additional: readonly SessionRequirements[]) {
      return access(runtime, target, allRequirements(requirements, ...additional), reader);
    },
    memberships<U, G>(membershipReader: MembershipReader<R, S, U, G>) {
      // A replacement reader must be selected on the unscoped base view.
      if (reader) throw new AuthConfigurationError("Membership reader is already configured");
      return access(runtime, target, requirements, membershipReader);
    },
    async enforce<U extends T>(
      actor: Actor<R, S, A> | null,
      resource: U,
      policy: Policy<PolicyContext<Actor<R, S, A>, U, M>>,
      options: AuthenticationOptions = {},
    ) {
      const proof = runtime.proof(actor, target);
      const principal = actor!;
      return runtime.run([proof.signal, options.signal], async (signal) => {
        // Revalidate credentials at the service boundary, including direct calls.
        const verified = await runtime.authenticate(proof.evidence, requirements, signal);
        if (
          !verified ||
          verified.subjectId !== principal.subjectId ||
          (verified.kind ?? "user") !== principal.kind
        ) {
          throw new AuthError("UNAUTHORIZED");
        }
        const membership = reader
          ? await reader(
              Object.freeze({ realmId: principal.realmId, subjectId: principal.subjectId }),
              resource,
              { signal },
            )
          : (undefined as M);
        signal.throwIfAborted();
        if (reader && membership == null) throw new AuthError("FORBIDDEN");
        const allowed = await policy({
          principal,
          resource,
          membership: membership as M,
          signal,
        });
        signal.throwIfAborted();
        if (allowed !== true) throw new AuthError("FORBIDDEN");
        runtime.checkEvidence(verified.session, requirements);
        return principal;
      });
    },
  });
}

export function createAuth<R extends string, E, S extends string>(
  definition: Realm<R, E, S>,
  options: { readonly now?: () => number } = {},
): Auth<R, E, S> {
  realm(definition.id, definition.source);
  const runtime = new Runtime(definition, options.now ?? Date.now);
  return Object.freeze({
    for<const A extends string>(target: Audience<A>) {
      identifier(target.id);
      return access<R, E, S, A, unknown, undefined>(runtime, target, allRequirements());
    },
    close: () => runtime.close(),
  });
}
