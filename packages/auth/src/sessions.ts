import { AuthConfigurationError, AuthError } from "./errors";
import type { SessionMutation, SessionRecord, SessionStore } from "./session-store";
import type { AuthSource, VerificationContext } from "./source";
import { snapshotVerification } from "./source";
export type { SessionStore, SessionRecord, SessionMutation } from "./session-store";

export interface SessionLifetime {
  readonly idle: number;
  readonly absolute: number;
  readonly renewAfter: number;
}

export function sessionLifetime(value: SessionLifetime): SessionLifetime {
  if (
    !value ||
    !Number.isSafeInteger(value.idle) ||
    !Number.isSafeInteger(value.absolute) ||
    !Number.isSafeInteger(value.renewAfter) ||
    value.renewAfter <= 0 ||
    value.idle <= 0 ||
    value.absolute <= 0 ||
    !(value.renewAfter < value.idle && value.idle <= value.absolute)
  ) {
    throw new AuthConfigurationError("Invalid session lifetime");
  }
  return Object.freeze({
    idle: value.idle,
    absolute: value.absolute,
    renewAfter: value.renewAfter,
  });
}

export interface ManagedSessionsOptions<E, S extends string> {
  readonly realmId: string;
  readonly login: AuthSource<E, S>;
  readonly store: SessionStore<S>;
  readonly lifetime: SessionLifetime;
  readonly subjectActive: (subjectId: S, context: VerificationContext) => Promise<boolean>;
  readonly now?: () => number;
}

export interface ManagedSession {
  readonly credential: string;
  readonly sessionId: string;
  readonly expiresAt: number;
}

const unauthorized = () => new AuthError("UNAUTHORIZED");
const unavailable = () => new AuthError("SERVICE_UNAVAILABLE");
const validTime = (n: unknown): n is number => Number.isSafeInteger(n) && (n as number) >= 0;
const tokenPattern = /^([0-9a-f-]{36})\.([A-Za-z0-9_-]{43})$/;

function aborted(signal: AbortSignal): void {
  signal.throwIfAborted();
}

function recordValid<S extends string>(
  r: SessionRecord<S>,
  id: string,
  realmId: string,
  now: number,
): boolean {
  return (
    !!r &&
    r.id === id &&
    r.realmId === realmId &&
    typeof r.subjectId === "string" &&
    !!r.subjectId.trim() &&
    r.subjectId.length <= 512 &&
    ["user", "guest", "service"].includes(r.kind) &&
    typeof r.tokenDigest === "string" &&
    /^[0-9a-f]{64}$/.test(r.tokenDigest) &&
    Number.isSafeInteger(r.revision) &&
    r.revision > 0 &&
    validTime(r.issuedAt) &&
    validTime(r.expiresAt) &&
    validTime(r.lastActiveAt) &&
    validTime(r.renewedAt) &&
    Number.isSafeInteger(r.idleTimeoutMs) &&
    r.idleTimeoutMs > 0 &&
    Number.isSafeInteger(r.renewAfterMs) &&
    r.renewAfterMs > 0 &&
    r.issuedAt <= r.renewedAt &&
    r.renewedAt <= r.lastActiveAt &&
    r.lastActiveAt <= now &&
    r.issuedAt < r.expiresAt &&
    Number.isSafeInteger(r.lastActiveAt + r.idleTimeoutMs) &&
    (r.authenticatedAt === null ||
      (validTime(r.authenticatedAt) && r.authenticatedAt <= r.issuedAt)) &&
    Array.isArray(r.assurance) &&
    r.assurance.every((x) => typeof x === "string" && !!x.trim() && x.length <= 128) &&
    (r.revokedAt === null || (validTime(r.revokedAt) && r.revokedAt >= r.issuedAt))
  );
}

export function createManagedSessions<E, S extends string>(options: ManagedSessionsOptions<E, S>) {
  if (!options.realmId?.trim()) throw new AuthConfigurationError("Invalid session realm");
  if (typeof options.subjectActive !== "function")
    throw new AuthConfigurationError("Invalid subject policy");
  if (options.login.realmId !== undefined && options.login.realmId !== options.realmId) {
    throw new AuthConfigurationError("Login source belongs to a different realm");
  }
  const lifetime = sessionLifetime(options.lifetime);
  const now = options.now ?? Date.now;
  const login = options.login;
  const store = options.store;
  const realmId = options.realmId;
  const subjectActive = options.subjectActive;
  const controller = new AbortController();
  const tasks = new Set<Promise<unknown>>();
  let stopping: Promise<void> | undefined;
  let stopped = false;
  const capabilities = Object.freeze({
    ...login.capabilities,
    assurance: Object.freeze([...(login.capabilities?.assurance ?? [])]),
  });
  const signalFor = (signal?: AbortSignal) => signal ?? new AbortController().signal;

  async function active(subject: S, signal: AbortSignal): Promise<void> {
    aborted(signal);
    let result: boolean;
    try {
      result = await subjectActive(subject, { signal });
    } catch {
      aborted(signal);
      throw unavailable();
    }
    aborted(signal);
    if (result !== true) throw unauthorized();
  }

  async function digest(value: string): Promise<string> {
    try {
      const bytes = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(value));
      return Array.from(new Uint8Array(bytes), (b) => b.toString(16).padStart(2, "0")).join("");
    } catch {
      throw unavailable();
    }
  }

  function clock(): number {
    let value: number;
    try {
      value = now();
    } catch {
      throw unavailable();
    }
    if (!validTime(value)) throw unavailable();
    return value;
  }

  async function lookup(credential: string, signal: AbortSignal) {
    aborted(signal);
    const match = typeof credential === "string" ? tokenPattern.exec(credential) : null;
    if (!match) throw unauthorized();
    let record: SessionRecord<S> | null;
    let tokenDigest: string;
    try {
      tokenDigest = await digest(credential);
      record = await store.read(realmId, match[1]!);
    } catch {
      aborted(signal);
      throw unavailable();
    }
    aborted(signal);
    if (record === null) throw unauthorized();
    const time = clock();
    if (
      !recordValid(record, match[1]!, realmId, time) ||
      record.tokenDigest !== tokenDigest ||
      record.revokedAt !== null
    )
      throw unauthorized();
    const expiry = Math.min(
      record.expiresAt,
      record.issuedAt + lifetime.absolute,
      record.lastActiveAt + Math.min(record.idleTimeoutMs, lifetime.idle),
    );
    if (!Number.isSafeInteger(expiry) || time >= expiry) throw unauthorized();
    await active(record.subjectId, signal);
    const checkedAt = clock();
    if (checkedAt >= expiry) throw unauthorized();
    return { record, tokenDigest, expiry, time: checkedAt };
  }

  async function sourceVerify(credential: string | null, context: VerificationContext) {
    aborted(context.signal);
    if (credential === null) return { status: "absent" as const };
    const { record, expiry } = await lookup(credential, context.signal);
    const result = {
      status: "verified" as const,
      subjectId: record.subjectId,
      kind: record.kind,
      session: {
        expiresAt: expiry,
        authoritative: true,
        sessionCreatedAt: record.issuedAt,
        ...(record.authenticatedAt === null ? {} : { authenticatedAt: record.authenticatedAt }),
        ...(record.assurance.length ? { assurance: [...record.assurance] } : {}),
      },
    };
    return result;
  }

  const source: AuthSource<string | null, S> = {
    realmId,
    capabilities: Object.freeze({
      authoritative: true,
      sessionCreatedAt: true,
      ...(capabilities.authenticatedAt ? { authenticatedAt: true } : {}),
      assurance: capabilities.assurance,
    }),
    verify: sourceVerify,
  };

  async function issue(evidence: E, opts: { signal?: AbortSignal } = {}): Promise<ManagedSession> {
    const signal = signalFor(opts.signal);
    aborted(signal);
    let outcome;
    try {
      outcome = await login.verify(evidence, { signal });
    } catch {
      aborted(signal);
      throw unavailable();
    }
    aborted(signal);
    if (!outcome || outcome.status !== "verified") throw unauthorized();
    const authentication = snapshotVerification(outcome);
    const verifiedAt = clock();
    if (
      !authentication ||
      authentication.status !== "verified" ||
      typeof authentication.subjectId !== "string" ||
      !authentication.subjectId.trim() ||
      authentication.subjectId.length > 512 ||
      (authentication.kind !== undefined &&
        !["user", "guest", "service"].includes(authentication.kind)) ||
      (authentication.session !== undefined &&
        (!validTime(authentication.session.expiresAt) ||
          authentication.session.expiresAt <= verifiedAt ||
          (authentication.session.sessionCreatedAt !== undefined &&
            (!validTime(authentication.session.sessionCreatedAt) ||
              authentication.session.sessionCreatedAt > verifiedAt)) ||
          (authentication.session.authenticatedAt !== undefined &&
            (!validTime(authentication.session.authenticatedAt) ||
              authentication.session.authenticatedAt > verifiedAt)) ||
          (authentication.session.assurance !== undefined &&
            (!Array.isArray(authentication.session.assurance) ||
              !authentication.session.assurance.every(
                (item) => typeof item === "string" && capabilities.assurance.includes(item),
              )))))
    ) {
      throw unauthorized();
    }
    await active(authentication.subjectId, signal);
    const issuedAt = clock();
    if (authentication.session && authentication.session.expiresAt <= issuedAt)
      throw unauthorized();
    const expiresAt = issuedAt + lifetime.absolute;
    if (!Number.isSafeInteger(expiresAt)) throw unavailable();
    let id: string, secret: string;
    try {
      id = crypto.randomUUID();
      const bytes = crypto.getRandomValues(new Uint8Array(32));
      secret = btoa(String.fromCharCode(...bytes))
        .replace(/\+/g, "-")
        .replace(/\//g, "_")
        .replace(/=+$/, "");
    } catch {
      throw unavailable();
    }
    const credential = `${id}.${secret}`;
    const tokenDigest = await digest(credential);
    const session = authentication.session;
    const authAt =
      capabilities.authenticatedAt &&
      session?.authenticatedAt !== undefined &&
      validTime(session.authenticatedAt)
        ? session.authenticatedAt
        : null;
    const assurance = [...(session?.assurance ?? [])];
    const record: SessionRecord<S> = {
      id,
      realmId,
      subjectId: authentication.subjectId,
      kind: authentication.kind ?? "user",
      tokenDigest,
      revision: 1,
      issuedAt,
      expiresAt,
      idleTimeoutMs: lifetime.idle,
      renewAfterMs: lifetime.renewAfter,
      lastActiveAt: issuedAt,
      renewedAt: issuedAt,
      authenticatedAt: authAt,
      assurance,
      revokedAt: null,
    };
    aborted(signal);
    if (session && session.expiresAt <= clock()) throw unauthorized();
    try {
      await store.create(record);
    } catch {
      aborted(signal);
      throw unavailable();
    }
    aborted(signal);
    if (session && session.expiresAt <= clock()) throw unauthorized();
    return { credential, sessionId: id, expiresAt: Math.min(expiresAt, issuedAt + lifetime.idle) };
  }

  async function mutate(
    kind: "touch" | "renew",
    credential: string,
    opts: { signal?: AbortSignal } = {},
  ) {
    const signal = signalFor(opts.signal);
    const found = await lookup(credential, signal);
    const { record } = found;
    const time = clock();
    if (
      kind === "renew" &&
      time < record.renewedAt + Math.max(record.renewAfterMs, lifetime.renewAfter)
    )
      throw unauthorized();
    let nextToken = credential;
    let nextDigest = found.tokenDigest;
    if (kind === "renew") {
      try {
        const secret = crypto.getRandomValues(new Uint8Array(32));
        nextToken = `${record.id}.${btoa(String.fromCharCode(...secret))
          .replace(/\+/g, "-")
          .replace(/\//g, "_")
          .replace(/=+$/, "")}`;
      } catch {
        throw unavailable();
      }
      nextDigest = await digest(nextToken);
    }
    const nextExpiry = Math.min(record.expiresAt, record.issuedAt + lifetime.absolute);
    const next: SessionRecord<S> = {
      ...record,
      tokenDigest: nextDigest,
      revision: record.revision + 1,
      expiresAt: nextExpiry,
      idleTimeoutMs: Math.min(record.idleTimeoutMs, lifetime.idle),
      renewAfterMs: Math.max(record.renewAfterMs, lifetime.renewAfter),
      lastActiveAt: time,
      ...(kind === "renew" ? { renewedAt: time } : {}),
    };
    const mutation: SessionMutation<S> = {
      kind,
      expectedRevision: record.revision,
      expectedDigest: found.tokenDigest,
      now: time,
      next,
    };
    aborted(signal);
    let changed: boolean;
    try {
      changed = await store.mutate(mutation);
    } catch {
      aborted(signal);
      throw unavailable();
    }
    aborted(signal);
    if (changed !== true) throw unauthorized();
    if (clock() >= Math.min(nextExpiry, time + next.idleTimeoutMs)) throw unauthorized();
    return {
      credential: nextToken,
      sessionId: record.id,
      expiresAt: Math.min(nextExpiry, time + next.idleTimeoutMs),
    };
  }

  function run<T>(
    operation: (signal: AbortSignal) => Promise<T>,
    signal?: AbortSignal,
  ): Promise<T> {
    if (stopped) return Promise.reject(unavailable());
    const combined = AbortSignal.any([controller.signal, ...(signal ? [signal] : [])]);
    const task = Promise.resolve().then(async () => {
      if (stopped) throw unavailable();
      aborted(combined);
      const value = await operation(combined);
      aborted(combined);
      return value;
    });
    tasks.add(task);
    void task.then(
      () => tasks.delete(task),
      () => tasks.delete(task),
    );
    return task;
  }

  return Object.freeze({
    source: Object.freeze({
      ...source,
      verify: (credential: string | null, context: VerificationContext) =>
        run((signal) => sourceVerify(credential, { ...context, signal }), context.signal),
    }),
    issue: (evidence: E, opts: { signal?: AbortSignal } = {}) =>
      run((signal) => issue(evidence, { signal }), opts.signal),
    async renew(credential: string, opts?: { signal?: AbortSignal }) {
      return run((signal) => mutate("renew", credential, { signal }), opts?.signal);
    },
    async touch(credential: string, opts?: { signal?: AbortSignal }): Promise<void> {
      await run((signal) => mutate("touch", credential, { signal }), opts?.signal);
    },
    async revoke(credential: string, opts: { signal?: AbortSignal } = {}): Promise<void> {
      await run(async (signal) => {
        aborted(signal);
        const match = typeof credential === "string" ? tokenPattern.exec(credential) : null;
        if (!match) throw unauthorized();
        let record: SessionRecord<S> | null;
        let tokenDigest: string;
        try {
          tokenDigest = await digest(credential);
          record = await store.read(realmId, match[1]!);
        } catch {
          aborted(signal);
          throw unavailable();
        }
        aborted(signal);
        if (
          record === null ||
          !recordValid(record, match[1]!, realmId, clock()) ||
          record.tokenDigest !== tokenDigest ||
          record.revokedAt !== null
        )
          throw unauthorized();
        const at = clock();
        let changed: boolean;
        try {
          changed = await store.revoke(realmId, record.id, at);
        } catch {
          aborted(signal);
          throw unavailable();
        }
        aborted(signal);
        if (changed !== true) throw unauthorized();
      }, opts.signal);
    },
    close(): Promise<void> {
      if (!stopping) {
        stopped = true;
        stopping = Promise.resolve().then(async () => {
          controller.abort(unavailable());
          await Promise.allSettled(tasks);
        });
      }
      return stopping;
    },
  });
}
