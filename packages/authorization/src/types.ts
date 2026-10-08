export type Awaitable<T> = T | Promise<T>;
export type Attributes = Readonly<Record<string, unknown>>;

/** Trusted application facts, not a credential or an authenticated JSON DTO. */
export interface Principal {
  readonly realmId: string;
  readonly subjectId: string;
  readonly kind: string;
  readonly attributes?: Attributes;
}

export interface Scope {
  readonly type: string;
  readonly id: string;
}

export interface Resource {
  readonly type: string;
  readonly id: string;
  readonly scope: Scope;
  readonly attributes?: Attributes;
}

export interface Permission<A extends string = string> {
  readonly action: A;
  readonly resourceType: string;
  readonly scope: Scope;
  /** Omission explicitly means every resource of this type in the exact scope. */
  readonly resourceId?: string;
}

export interface CredentialLimit<A extends string = string> {
  readonly permissions: readonly Permission<A>[];
  readonly expiresAt?: number;
}

export interface Request<A extends string = string, R extends Resource = Resource, C = Attributes> {
  readonly principal: Principal | null;
  readonly action: A;
  readonly resource: R;
  readonly context: C;
  readonly audience?: string;
  readonly credential?: CredentialLimit<A>;
}

export type Effect = "allow" | "deny" | "abstain";
export type ReasonCode =
  | "ALLOWED"
  | "DEFAULT_DENY"
  | "EXPLICIT_DENY"
  | "BOUNDARY_DENIED"
  | "UNKNOWN_ACTION"
  | "EVALUATION_FAILED"
  | "TIMEOUT"
  | "CANCELLED";

export interface Decision {
  readonly effect: "allow" | "deny";
  readonly code: ReasonCode;
}

export interface Evaluation {
  readonly signal: AbortSignal;
  readonly now: number;
}

export type Predicate<A extends string = string, R extends Resource = Resource, C = Attributes> = (
  request: Request<A, R, C>,
  evaluation: Evaluation,
) => Awaitable<boolean>;

export type Condition<A extends string = string, R extends Resource = Resource, C = Attributes> =
  | { readonly kind: "all"; readonly conditions: readonly Condition<A, R, C>[] }
  | { readonly kind: "any"; readonly conditions: readonly Condition<A, R, C>[] }
  | { readonly kind: "predicate"; readonly test: Predicate<A, R, C> }
  | {
      readonly kind: "attribute";
      readonly source: "principal" | "resource" | "context";
      readonly key: string;
      readonly operator: "equals" | "in";
      readonly value:
        | string
        | number
        | boolean
        | null
        | readonly (string | number | boolean | null)[];
    }
  | { readonly kind: "relation"; readonly relation: string; readonly target?: Resource };

export interface Rule<A extends string = string, R extends Resource = Resource, C = Attributes> {
  /** Local identifier; explanations return ordinal paths, never this value. */
  readonly id: string;
  readonly effect: "allow" | "deny";
  readonly actions: readonly A[];
  readonly resourceType: string;
  readonly scope?: Scope;
  readonly resourceId?: string;
  readonly when: Condition<A, R, C>;
}

export interface Policy<A extends string = string, R extends Resource = Resource, C = Attributes> {
  evaluate(request: Request<A, R, C>, evaluation: Evaluation): Awaitable<Effect>;
}

export interface RelationResolver {
  check(
    principal: Principal,
    relation: string,
    resource: Resource,
    evaluation: Evaluation,
  ): Awaitable<boolean>;
}

export interface ResolvedAttributes<C = Attributes> {
  readonly principal?: Attributes;
  readonly resource?: Attributes;
  readonly context?: C;
}

export interface AttributeResolver<
  A extends string = string,
  R extends Resource = Resource,
  C = Attributes,
> {
  resolve(request: Request<A, R, C>, evaluation: Evaluation): Awaitable<ResolvedAttributes<C>>;
}

export interface AuthorizationOptions<A extends string, R extends Resource, C> {
  readonly actions: readonly A[];
  readonly rules?: readonly Rule<A, R, C>[];
  readonly policies?: readonly Policy<A, R, C>[];
  /** These intersect before any allow-producing extension executes. */
  readonly boundaries?: readonly Predicate<A, R, C>[];
  readonly identity?: {
    readonly required?: boolean;
    readonly realms?: readonly string[];
    readonly audiences?: readonly string[];
    readonly credentialRequired?: boolean;
  };
  readonly relations?: RelationResolver;
  readonly attributes?: AttributeResolver<A, R, C>;
  readonly resolveResource?: (resource: R, evaluation: Evaluation) => Awaitable<R>;
  readonly timeoutMs?: number;
  readonly clock?: () => number;
  /** No subject, resource IDs, policy data or exception messages are supplied. */
  readonly observe?: (decision: Decision) => Awaitable<void>;
  readonly explain?: {
    readonly action: A;
    readonly resource: R;
  };
}

export interface Explanation {
  readonly decision: Decision;
  readonly paths: readonly string[];
}

export interface Authorization<
  A extends string = string,
  R extends Resource = Resource,
  C = Attributes,
> {
  check(request: Request<A, R, C>, options?: { signal?: AbortSignal }): Promise<Decision>;
  can(request: Request<A, R, C>, options?: { signal?: AbortSignal }): Promise<boolean>;
  enforce(request: Request<A, R, C>, options?: { signal?: AbortSignal }): Promise<void>;
  explain(
    manager: Request<A, R, C>,
    target: Request<A, R, C>,
    options?: { signal?: AbortSignal },
  ): Promise<Explanation>;
}

export interface Role<A extends string = string> {
  readonly id: string;
  readonly scope: Scope;
  readonly permissions: readonly Permission<A>[];
  /** IDs resolve only within this role's exact scope. */
  readonly inherits?: readonly string[];
}

export interface Binding {
  readonly id: string;
  readonly principal: Principal;
  readonly roleId: string;
  readonly scope: Scope;
  readonly expiresAt?: number;
}

export interface RoleGraph<A extends string = string> {
  readonly roles: readonly Role<A>[];
  readonly bindings: readonly Binding[];
}

export interface RoleSnapshot<A extends string = string> {
  readonly revision: string;
  readonly graph: RoleGraph<A>;
}

/** Trusted internal repository. Mutations belong behind the authorized management service. */
export interface RoleStore<A extends string = string> {
  read(evaluation: Evaluation): Awaitable<RoleSnapshot<A>>;
  compareAndSwap(
    expectedRevision: string,
    next: RoleSnapshot<A>,
    evaluation: Evaluation,
  ): Awaitable<boolean>;
}
