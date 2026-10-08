import { createHash, randomBytes, randomUUID, timingSafeEqual } from "node:crypto";
import type { ApiKeyStore, KeyMetadata, KeyRecord, KeySubject } from "./store";

export type { ApiKeyStore, KeyMetadata, KeyRecord, KeyRotation, KeySubject } from "./store";

export type ApiKeyErrorCode = "INVALID_INPUT" | "FORBIDDEN" | "CONFLICT" | "UNAVAILABLE";

export class ApiKeyError extends Error {
  constructor(readonly code: ApiKeyErrorCode) {
    super(
      {
        INVALID_INPUT: "Invalid API key input",
        FORBIDDEN: "API key operation forbidden",
        CONFLICT: "API key operation conflicts with current state",
        UNAVAILABLE: "API key service unavailable",
      }[code],
    );
    this.name = "ApiKeyError";
  }
}

export interface ApiKeyConfig {
  readonly maxLifetimeMs: number;
  readonly maxOverlapMs: number;
}

export function apiKeyConfig(config: ApiKeyConfig): ApiKeyConfig {
  if (
    !Number.isSafeInteger(config.maxLifetimeMs) ||
    config.maxLifetimeMs <= 0 ||
    !Number.isSafeInteger(config.maxOverlapMs) ||
    config.maxOverlapMs < 0 ||
    config.maxOverlapMs > config.maxLifetimeMs
  ) {
    throw new ApiKeyError("INVALID_INPUT");
  }
  return Object.freeze({
    maxLifetimeMs: config.maxLifetimeMs,
    maxOverlapMs: config.maxOverlapMs,
  });
}

export interface IssueKeyInput {
  readonly subject: KeySubject;
  readonly requestedScopes: readonly string[];
  readonly expiresAt: number;
  readonly requestId: string;
}

export interface KeyQuery {
  readonly subject: KeySubject;
  readonly id: string;
}

export interface RotateKeyInput extends KeyQuery {
  readonly expectedRevision: number;
  readonly overlapMs: number;
}

export interface ListKeysInput {
  readonly subject: KeySubject;
  readonly after?: string;
  readonly limit?: number;
}

export interface IssuedKey {
  readonly key: KeyMetadata;
  /** Only returned by the winning creation/rotation. Never persisted or recoverable. */
  readonly credential: string | null;
  readonly replayed: boolean;
}

export type ManagementAction = "issue" | "list" | "read" | "rotate" | "revoke";

export interface ManagementTarget {
  readonly subject: KeySubject;
  readonly key: KeyMetadata | null;
  readonly requestedScopes: readonly string[];
}

export interface ApiKeyOptions<C, R> {
  readonly store: ApiKeyStore;
  readonly config: ApiKeyConfig;
  /** Trusted application code must validate caller provenance and independent
   * management/delegation permission. JSON identity claims are not callers.
   */
  readonly authorizeManagement: (
    caller: C,
    action: ManagementAction,
    target: ManagementTarget,
  ) => boolean | Promise<boolean>;
  /** Trusted delegation policy, not the client's requested permissions. */
  readonly grantScopes: (
    caller: C,
    subject: KeySubject,
    requested: readonly string[],
  ) => readonly string[] | Promise<readonly string[]>;
  readonly subjectActive: (subject: KeySubject) => boolean | Promise<boolean>;
  /** Read current subject permission, membership and resource policy on every use. */
  readonly authorizeUse: (
    key: KeyMetadata,
    scope: string,
    resource: R,
  ) => boolean | Promise<boolean>;
  readonly now?: () => number;
}

function identifier(value: unknown): asserts value is string {
  if (typeof value !== "string" || !value.trim() || value.length > 256) {
    throw new ApiKeyError("INVALID_INPUT");
  }
}

function subjectSnapshot(value: KeySubject): KeySubject {
  if (!value) throw new ApiKeyError("INVALID_INPUT");
  identifier(value.namespace);
  identifier(value.tenantId);
  identifier(value.subjectId);
  return Object.freeze({
    namespace: value.namespace,
    tenantId: value.tenantId,
    subjectId: value.subjectId,
  });
}

function scopesSnapshot(value: readonly string[]): readonly string[] {
  if (!Array.isArray(value) || value.length > 128) throw new ApiKeyError("INVALID_INPUT");
  for (const scope of value) identifier(scope);
  return Object.freeze([...new Set(value)].sort());
}

export function sameKeySubject(a: KeySubject, b: KeySubject): boolean {
  return a.namespace === b.namespace && a.tenantId === b.tenantId && a.subjectId === b.subjectId;
}

/** Bounded namespace-aware identity, when all sources use the same application contract. */
export function namespacedSubjectId(subject: KeySubject): string {
  const canonical = JSON.stringify([subject.namespace, subject.tenantId, subject.subjectId]);
  return `key-subject:sha256:${hash(canonical)}`;
}

function metadata(record: KeyMetadata): KeyMetadata {
  return Object.freeze({
    id: record.id,
    subject: subjectSnapshot(record.subject),
    scopes: scopesSnapshot(record.scopes),
    revision: record.revision,
    issuedAt: record.issuedAt,
    expiresAt: record.expiresAt,
    revokedAt: record.revokedAt,
    overlapUntil: record.overlapUntil,
  });
}

const hash = (secret: string) => createHash("sha256").update(secret).digest("hex");

function matches(secret: string, digest: string | null): boolean {
  // Both buffers have a fixed 32-byte length. Never compare secret strings.
  const actual = Buffer.from(hash(secret), "hex");
  const expected = Buffer.from(
    digest && /^[a-f0-9]{64}$/.test(digest) ? digest : "0".repeat(64),
    "hex",
  );
  const equal = timingSafeEqual(actual, expected);
  return digest !== null && equal;
}

function credential(id: string) {
  const secret = randomBytes(32).toString("base64url");
  return { credential: `lk_${id}.${secret}`, digest: hash(secret) };
}

export function createApiKeys<C, R>(options: ApiKeyOptions<C, R>) {
  const config = apiKeyConfig(options.config);
  const clock = options.now ?? Date.now;
  const tasks = new Set<Promise<unknown>>();
  let stopped = false;
  let stopping: Promise<void> | undefined;

  function now() {
    const value = clock();
    if (!Number.isSafeInteger(value) || value < 0) throw new ApiKeyError("UNAVAILABLE");
    return value;
  }

  function run<T>(work: () => Promise<T>): Promise<T> {
    const task = Promise.resolve().then(async () => {
      if (stopped) throw new ApiKeyError("UNAVAILABLE");
      try {
        return await work();
      } catch (error) {
        if (error instanceof ApiKeyError) throw new ApiKeyError(error.code);
        // Provider and policy errors can contain SQL parameters or credentials.
        throw new ApiKeyError("UNAVAILABLE");
      }
    });
    tasks.add(task);
    void task.then(
      () => tasks.delete(task),
      () => tasks.delete(task),
    );
    return task;
  }

  async function authorize(
    caller: C,
    action: ManagementAction,
    subject: KeySubject,
    key: KeyMetadata | null = null,
    requestedScopes: readonly string[] = [],
  ) {
    if (
      (await options.authorizeManagement(
        caller,
        action,
        Object.freeze({ subject, key, requestedScopes }),
      )) !== true
    ) {
      throw new ApiKeyError("FORBIDDEN");
    }
  }

  async function owned(input: KeyQuery): Promise<KeyRecord | null> {
    const record = await options.store.read(input.id);
    return record && sameKeySubject(record.subject, input.subject) ? record : null;
  }

  async function verify(opaque: string | null): Promise<KeyMetadata | null> {
    if (typeof opaque !== "string") return null;
    const parsed =
      /^lk_([a-f0-9]{8}-[a-f0-9]{4}-4[a-f0-9]{3}-[89ab][a-f0-9]{3}-[a-f0-9]{12})\.([A-Za-z0-9_-]{43})$/.exec(
        opaque,
      );
    if (!parsed) return null;
    const record = await options.store.read(parsed[1]);
    if (!record) return null;
    const current = matches(parsed[2], record.digest);
    const previous = matches(parsed[2], record.previousDigest);
    const key = metadata(record);
    if (
      key.revokedAt !== null ||
      key.expiresAt <= now() ||
      (!current && !(previous && key.overlapUntil !== null && key.overlapUntil > now())) ||
      (await options.subjectActive(key.subject)) !== true
    )
      return null;
    // Activity checks may block while another caller revokes or rotates the row.
    const latest = await options.store.read(parsed[1]);
    if (!latest || !sameKeySubject(latest.subject, key.subject)) return null;
    const currentNow = matches(parsed[2], latest.digest);
    const previousNow = matches(parsed[2], latest.previousDigest);
    const at = now();
    if (
      latest.revokedAt !== null ||
      latest.expiresAt <= at ||
      (!currentNow && !(previousNow && (latest.overlapUntil ?? 0) > at))
    )
      return null;
    return metadata(latest);
  }

  const service = {
    issue(input: IssueKeyInput, caller: C): Promise<IssuedKey> {
      const subject = subjectSnapshot(input.subject);
      const requested = scopesSnapshot(input.requestedScopes);
      identifier(input.requestId);
      const expiresAt = input.expiresAt;
      const requestId = input.requestId;
      return run(async () => {
        const issuedAt = now();
        if (
          !Number.isSafeInteger(expiresAt) ||
          expiresAt <= issuedAt ||
          expiresAt - issuedAt > config.maxLifetimeMs
        )
          throw new ApiKeyError("INVALID_INPUT");
        await authorize(caller, "issue", subject, null, requested);
        const granted = scopesSnapshot(await options.grantScopes(caller, subject, requested));
        if (granted.some((scope) => !requested.includes(scope))) throw new ApiKeyError("FORBIDDEN");
        if ((await options.subjectActive(subject)) !== true) throw new ApiKeyError("FORBIDDEN");
        const id = randomUUID();
        const generated = credential(id);
        const outcome = await options.store.create({
          id,
          subject,
          scopes: granted,
          requestId,
          digest: generated.digest,
          previousDigest: null,
          overlapUntil: null,
          revision: 0,
          issuedAt,
          expiresAt,
          revokedAt: null,
        });
        const key = metadata(outcome.record);
        if (
          !sameKeySubject(key.subject, subject) ||
          key.expiresAt !== expiresAt ||
          JSON.stringify(key.scopes) !== JSON.stringify(granted)
        )
          throw new ApiKeyError("CONFLICT");
        return Object.freeze({
          key,
          credential: outcome.created ? generated.credential : null,
          replayed: !outcome.created,
        });
      });
    },
    list(input: ListKeysInput, caller: C): Promise<readonly KeyMetadata[]> {
      const subject = subjectSnapshot(input.subject);
      const after = input.after ?? null;
      if (after !== null) identifier(after);
      const limit = input.limit ?? 50;
      if (!Number.isInteger(limit) || limit < 1 || limit > 100)
        throw new ApiKeyError("INVALID_INPUT");
      return run(async () => {
        await authorize(caller, "list", subject);
        const records = await options.store.list(subject, after, limit);
        if (records.some((row) => !sameKeySubject(row.subject, subject))) {
          throw new ApiKeyError("UNAVAILABLE");
        }
        return Object.freeze(records.map(metadata));
      });
    },
    read(input: KeyQuery, caller: C): Promise<KeyMetadata | null> {
      const query = { subject: subjectSnapshot(input.subject), id: input.id };
      identifier(query.id);
      return run(async () => {
        // Authorize the target partition before loading any private metadata.
        await authorize(caller, "read", query.subject);
        const record = await owned(query);
        if (!record) return null;
        const key = metadata(record);
        await authorize(caller, "read", query.subject, key);
        return key;
      });
    },
    rotate(input: RotateKeyInput, caller: C): Promise<IssuedKey> {
      const query = { subject: subjectSnapshot(input.subject), id: input.id };
      identifier(query.id);
      const { expectedRevision, overlapMs } = input;
      if (
        !Number.isSafeInteger(expectedRevision) ||
        expectedRevision < 0 ||
        !Number.isSafeInteger(overlapMs) ||
        overlapMs < 0 ||
        overlapMs > config.maxOverlapMs
      )
        throw new ApiKeyError("INVALID_INPUT");
      return run(async () => {
        await authorize(caller, "rotate", query.subject);
        const record = await owned(query);
        if (!record) throw new ApiKeyError("CONFLICT");
        await authorize(caller, "rotate", query.subject, metadata(record));
        if ((await options.subjectActive(query.subject)) !== true)
          throw new ApiKeyError("FORBIDDEN");
        const generated = credential(query.id);
        const next = await options.store.rotate({
          ...query,
          expectedRevision,
          overlapMs,
          digest: generated.digest,
          now: now(),
        });
        if (!next) throw new ApiKeyError("CONFLICT");
        return Object.freeze({
          key: metadata(next),
          credential: generated.credential,
          replayed: false,
        });
      });
    },
    revoke(input: KeyQuery, caller: C): Promise<boolean> {
      const query = { subject: subjectSnapshot(input.subject), id: input.id };
      identifier(query.id);
      return run(async () => {
        await authorize(caller, "revoke", query.subject);
        const record = await owned(query);
        if (!record) return false;
        await authorize(caller, "revoke", query.subject, metadata(record));
        return options.store.revoke(query.subject, query.id, now());
      });
    },
    verify(opaque: string | null): Promise<KeyMetadata | null> {
      return run(() => verify(opaque));
    },
    use(opaque: string | null, scope: string, resource: R): Promise<KeyMetadata> {
      identifier(scope);
      return run(async () => {
        const key = await verify(opaque);
        if (
          !key ||
          !key.scopes.includes(scope) ||
          (await options.authorizeUse(key, scope, resource)) !== true
        )
          throw new ApiKeyError("FORBIDDEN");
        // No positive validation cache, including after an asynchronous policy read.
        const current = await verify(opaque);
        if (!current || !current.scopes.includes(scope)) throw new ApiKeyError("FORBIDDEN");
        return current;
      });
    },
    close(): Promise<void> {
      if (!stopping) {
        stopped = true;
        stopping = Promise.resolve().then(async () => {
          await Promise.allSettled(tasks);
        });
      }
      return stopping;
    },
  };
  return Object.freeze(service);
}

export type ApiKeys<C, R> = ReturnType<typeof createApiKeys<C, R>>;
