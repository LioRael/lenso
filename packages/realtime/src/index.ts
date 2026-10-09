// Registry snapshots isolate fan-out and cleanup from reentrant host callbacks.
/* oxlint-disable unicorn/no-useless-spread */
export type * from "./contracts";
export { RealtimeError } from "./contracts";
export { resolveRealtimeConfig } from "./config";

import {
  RealtimeError,
  type Cursor,
  type Delivery,
  type Envelope,
  type Identity,
  type Realtime,
  type RealtimeConnection,
  type RealtimeOptions,
  type Resource,
  type Subscription,
} from "./contracts";
import { resolveRealtimeConfig } from "./config";
import { cursorToken, parseCursor, part, resourceTopic, sameCursor, serialize } from "./wire";

type CloseReason = "revoked" | "deleted" | "expired" | "shutdown" | "unsubscribed";
interface SubState {
  handle: Subscription;
  topic: string;
  connection: ConnectionState;
  abort: AbortController;
  validUntil: number;
  cursor?: Cursor;
  initializing: boolean;
  renewing: boolean;
}
interface ConnectionState {
  identity: Identity;
  abort: AbortController;
  subscriptions: Map<string, SubState>;
  queue: { text: string; bytes: number }[];
  bytes: number;
  terminal?: Envelope;
  waiter?: (value: IteratorResult<Envelope>) => void;
  closed: boolean;
  consumed: boolean;
  heartbeatAt: number;
  removeAbort: () => void;
}

export async function createRealtime<P>(options: RealtimeOptions<P>): Promise<Realtime<P>> {
  const config = resolveRealtimeConfig(options.config);
  const provider = options.provider;
  const connections = new Set<ConnectionState>();
  const topics = new Map<string, Set<SubState>>();
  let stopped = false;
  let failed = false;
  let pending = 0;
  let closing: Promise<void> | undefined;
  let sweep: ReturnType<typeof setInterval> | undefined;
  const lifetime = new AbortController();
  const diagnostic = (event: "overflow" | "provider" | "authorization") => {
    try {
      options.onDiagnostic?.(event);
    } catch {}
  };
  const assertOpen = () => {
    if (failed) throw new RealtimeError("provider");
    if (stopped) throw new RealtimeError("closed");
  };
  const live = (connection: ConnectionState) => {
    assertOpen();
    if (connection.closed) throw new RealtimeError("closed");
    if (connection.identity.expiresAt <= Date.now()) {
      closeConnection(connection, { version: 1, kind: "closed", reason: "expired" });
      throw new RealtimeError("expired");
    }
  };
  const subjectConnections = (identity: Identity) =>
    [...connections].filter(
      (c) => c.identity.scope === identity.scope && c.identity.subject === identity.subject,
    );
  const purge = (connection: ConnectionState, subscription: string) => {
    connection.queue = connection.queue.filter(
      (item) => JSON.parse(item.text).subscription !== subscription,
    );
    connection.bytes = connection.queue.reduce((sum, item) => sum + item.bytes, 0);
  };
  const removeSub = (sub: SubState, reason: CloseReason, notify = true) => {
    if (!sub.connection.subscriptions.delete(sub.handle.id)) return;
    sub.abort.abort(new RealtimeError("closed"));
    const listeners = topics.get(sub.topic);
    listeners?.delete(sub);
    if (!listeners?.size) topics.delete(sub.topic);
    purge(sub.connection, sub.handle.id);
    if (notify)
      push(sub.connection, { version: 1, kind: "closed", subscription: sub.handle.id, reason });
  };
  function closeConnection(connection: ConnectionState, terminal?: Envelope) {
    if (connection.closed) return;
    connection.closed = true;
    connections.delete(connection);
    connection.removeAbort();
    connection.abort.abort(new RealtimeError("closed"));
    for (const sub of [...connection.subscriptions.values()]) removeSub(sub, "shutdown", false);
    connection.queue = [];
    connection.bytes = 0;
    connection.terminal = terminal;
    if (connection.waiter) {
      const waiter = connection.waiter;
      connection.waiter = undefined;
      connection.terminal = undefined;
      waiter(terminal ? { done: false, value: terminal } : { done: true, value: undefined });
    }
  }
  function push(connection: ConnectionState, event: Envelope) {
    if (connection.closed) return;
    if (connection.identity.expiresAt <= Date.now()) {
      closeConnection(connection, { version: 1, kind: "closed", reason: "expired" });
      return;
    }
    // Data is validated at the provider boundary. Envelope metadata must not
    // consume the payload's structural budget a second time.
    const text = JSON.stringify(event);
    if (new TextEncoder().encode(text).byteLength > config.maxPayloadBytes + 512) {
      diagnostic("overflow");
      closeConnection(connection, { version: 1, kind: "gap", reason: "overflow" });
      return;
    }
    if (connection.waiter) {
      const waiter = connection.waiter;
      connection.waiter = undefined;
      waiter({ done: false, value: JSON.parse(text) });
      return;
    }
    const bytes = new TextEncoder().encode(text).byteLength;
    if (
      connection.queue.length >= config.maxBufferedEvents ||
      connection.bytes + bytes > config.maxBufferedBytes
    ) {
      diagnostic("overflow");
      closeConnection(connection, { version: 1, kind: "gap", reason: "overflow" });
      return;
    }
    connection.queue.push({ text, bytes });
    connection.bytes += bytes;
  }
  const providerFailed = () => {
    if (failed || stopped) return;
    failed = true;
    clearInterval(sweep);
    lifetime.abort(new RealtimeError("provider"));
    diagnostic("provider");
    for (const connection of [...connections])
      closeConnection(connection, { version: 1, kind: "gap", reason: "provider" });
    void provider.close().catch(() => {});
  };
  const receive = (delivery: Delivery) => {
    if (failed || stopped) return;
    const listeners = topics.get(delivery.topic);
    if (!listeners) return;
    // A differently configured publisher can exceed this instance's limit.
    // Reject before advancing any subscriber cursor; never throw from callbacks.
    serialize(delivery.event, config.maxPayloadBytes);
    const token = cursorToken(delivery.cursor);
    for (const sub of [...listeners]) {
      if (delivery.event.kind === "revoke" || delivery.event.kind === "deleted") {
        removeSub(sub, delivery.event.kind === "revoke" ? "revoked" : "deleted");
        continue;
      }
      if (sub.initializing) continue; // ready always requires a fresh snapshot
      if (sub.validUntil <= Date.now()) {
        removeSub(sub, "expired");
        continue;
      }
      const previous = sub.cursor!;
      const next = delivery.cursor;
      if (sameCursor(previous, next)) continue;
      let reason: Envelope["reason"];
      if (next.generation !== previous.generation) reason = "generation";
      else if (next.sequence < previous.sequence) reason = "out-of-order";
      else if (next.sequence !== previous.sequence + 1) reason = "sequence";
      if (reason)
        push(sub.connection, {
          version: 1,
          kind: "gap",
          subscription: sub.handle.id,
          reason,
          cursor: token,
        });
      if (reason === "out-of-order") continue;
      sub.cursor = { ...next };
      push(sub.connection, {
        version: 1,
        kind: "update",
        subscription: sub.handle.id,
        cursor: token,
        type: delivery.event.type,
        data: delivery.event.data,
      });
    }
  };
  const deliver = (delivery: Delivery) => {
    try {
      receive(delivery);
    } catch {
      providerFailed();
    }
  };
  try {
    await provider.start(deliver, providerFailed);
    assertOpen();
  } catch {
    await provider.close().catch(() => {});
    throw new RealtimeError("provider");
  }

  // Keep a slot until the underlying operation settles, even after timeout.
  // An uncooperative authorizer cannot create unbounded abandoned work.
  const run = async <T>(
    signal: AbortSignal,
    operation: (signal: AbortSignal) => Promise<T>,
  ): Promise<T> => {
    assertOpen();
    if (signal.aborted) throw new RealtimeError("closed");
    if (pending >= config.maxPendingOperations) throw new RealtimeError("limit");
    pending++;
    const abort = new AbortController();
    const cancelled = () => abort.abort(new RealtimeError("closed"));
    const shutdown = () => abort.abort(new RealtimeError(failed ? "provider" : "closed"));
    signal.addEventListener("abort", cancelled, { once: true });
    lifetime.signal.addEventListener("abort", shutdown, { once: true });
    const timer = setTimeout(
      () => abort.abort(new RealtimeError("expired")),
      config.snapshotTimeoutMs,
    );
    let rejectAbort!: (reason: unknown) => void;
    const interrupted = new Promise<never>((_, reject) => {
      rejectAbort = reject;
    });
    const onAbort = () => rejectAbort(abort.signal.reason);
    abort.signal.addEventListener("abort", onAbort, { once: true });
    const work = Promise.resolve()
      .then(() => operation(abort.signal))
      .finally(() => {
        pending--;
      });
    try {
      return await Promise.race([work, interrupted]);
    } finally {
      clearTimeout(timer);
      signal.removeEventListener("abort", cancelled);
      lifetime.signal.removeEventListener("abort", shutdown);
      abort.signal.removeEventListener("abort", onAbort);
    }
  };
  const current = async (topic: string): Promise<Cursor> => {
    try {
      return await provider.current(topic);
    } catch (error) {
      if (error instanceof RealtimeError && error.code === "limit") throw error;
      providerFailed();
      throw new RealtimeError("provider");
    }
  };
  const authorize = async (sub: SubState) => {
    const startedAt = Date.now();
    let result: { validUntil: number } | false;
    try {
      result = await run(sub.abort.signal, (signal) =>
        options.authorize(sub.connection.identity as Identity<P>, sub.handle.resource, signal),
      );
    } catch (error) {
      diagnostic("authorization");
      if (error instanceof RealtimeError) throw error;
      throw new RealtimeError("denied");
    }
    live(sub.connection);
    if (!sub.connection.subscriptions.has(sub.handle.id)) throw new RealtimeError("closed");
    const now = Date.now();
    if (!result || !Number.isFinite(result.validUntil) || result.validUntil <= now)
      throw new RealtimeError("denied");
    sub.validUntil = Math.min(
      result.validUntil,
      startedAt + config.authorizationLeaseMs,
      sub.connection.identity.expiresAt,
    );
    if (sub.validUntil <= now) throw new RealtimeError("expired");
  };
  const next = async (connection: ConnectionState): Promise<IteratorResult<Envelope>> => {
    for (const sub of [...connection.subscriptions.values()])
      if (!sub.initializing && sub.validUntil <= Date.now()) removeSub(sub, "expired");
    if (!connection.closed && connection.identity.expiresAt <= Date.now())
      closeConnection(connection, { version: 1, kind: "closed", reason: "expired" });
    const item = connection.queue.shift();
    if (item) {
      connection.bytes -= item.bytes;
      return { done: false, value: JSON.parse(item.text) };
    }
    if (connection.terminal) {
      const value = connection.terminal;
      connection.terminal = undefined;
      return { done: false, value };
    }
    if (connection.closed) return { done: true, value: undefined };
    if (connection.waiter) throw new RealtimeError("invalid-input");
    return new Promise((resolve) => {
      connection.waiter = resolve;
    });
  };
  sweep = setInterval(() => {
    const now = Date.now();
    for (const connection of [...connections]) {
      if (connection.identity.expiresAt <= now) {
        closeConnection(connection, { version: 1, kind: "closed", reason: "expired" });
        continue;
      }
      for (const sub of [...connection.subscriptions.values()])
        if (!sub.initializing && sub.validUntil <= now) removeSub(sub, "expired");
      if (now - connection.heartbeatAt >= config.heartbeatMs) {
        connection.heartbeatAt = now;
        push(connection, { version: 1, kind: "heartbeat" });
      }
    }
  }, config.sweepMs);

  const publish = async (resource: Resource, event: Delivery["event"]) => {
    assertOpen();
    const topic = resourceTopic(resource);
    const copy = JSON.parse(serialize(event, config.maxPayloadBytes)) as Delivery["event"];
    try {
      return cursorToken(
        await run(new AbortController().signal, () => provider.publish(topic, copy)),
      );
    } catch (error) {
      if (error instanceof RealtimeError && (error.code === "limit" || error.code === "payload"))
        throw error;
      providerFailed();
      throw new RealtimeError("provider");
    }
  };
  return {
    connect(identity, { signal } = {}) {
      assertOpen();
      part(identity.scope);
      part(identity.subject);
      if (!Number.isFinite(identity.expiresAt) || identity.expiresAt <= Date.now())
        throw new RealtimeError("expired");
      if (signal?.aborted) throw new RealtimeError("closed");
      if (
        connections.size >= config.maxConnections ||
        subjectConnections(identity).length >= config.maxConnectionsPerSubject
      )
        throw new RealtimeError("limit");
      const connection: ConnectionState = {
        identity: Object.freeze({ ...identity }),
        abort: new AbortController(),
        subscriptions: new Map(),
        queue: [],
        bytes: 0,
        closed: false,
        consumed: false,
        heartbeatAt: Date.now(),
        removeAbort: () => {},
      };
      const disconnect = () => closeConnection(connection);
      signal?.addEventListener("abort", disconnect, { once: true });
      connection.removeAbort = () => signal?.removeEventListener("abort", disconnect);
      connections.add(connection);
      const api: RealtimeConnection = {
        async subscribe(input, subscriptionOptions = {}) {
          live(connection);
          const resource = Object.freeze({ scope: input.scope, type: input.type, id: input.id });
          const topic = resourceTopic(resource);
          if (resource.scope !== connection.identity.scope) throw new RealtimeError("denied");
          const subjectCount = subjectConnections(connection.identity).reduce(
            (sum, c) => sum + c.subscriptions.size,
            0,
          );
          if (
            connection.subscriptions.size >= config.maxSubscriptionsPerConnection ||
            subjectCount >= config.maxSubscriptionsPerSubject ||
            (topics.get(topic)?.size ?? 0) >= config.maxSubscribersPerTopic ||
            (!topics.has(topic) && topics.size >= config.maxTopics)
          )
            throw new RealtimeError("limit");
          const id = crypto.randomUUID();
          const sub: SubState = {
            topic,
            connection,
            abort: new AbortController(),
            validUntil: 0,
            initializing: true,
            renewing: false,
            handle: Object.freeze({
              id,
              resource,
              unsubscribe: () => removeSub(sub, "unsubscribed"),
              async renew() {
                live(connection);
                if (!connection.subscriptions.has(id)) throw new RealtimeError("closed");
                if (sub.validUntil <= Date.now()) {
                  removeSub(sub, "expired");
                  throw new RealtimeError("expired");
                }
                if (sub.renewing) throw new RealtimeError("limit");
                sub.renewing = true;
                try {
                  await authorize(sub);
                } catch (error) {
                  removeSub(sub, "revoked");
                  throw error;
                } finally {
                  sub.renewing = false;
                }
              },
            }),
          };
          connection.subscriptions.set(id, sub);
          if (!topics.has(topic)) topics.set(topic, new Set());
          topics.get(topic)!.add(sub);
          try {
            await authorize(sub);
            sub.cursor = await run(sub.abort.signal, () => current(topic));
            live(connection);
            if (!connection.subscriptions.has(id) || sub.validUntil <= Date.now())
              throw new RealtimeError("expired");
            sub.initializing = false;
            push(connection, {
              version: 1,
              kind: "ready",
              subscription: id,
              cursor: cursorToken(sub.cursor),
            });
            if (subscriptionOptions.cursor !== undefined) {
              const parsed = parseCursor(subscriptionOptions.cursor);
              const age = parsed ? Date.now() - parsed.issuedAt : Infinity;
              push(connection, {
                version: 1,
                kind: "gap",
                subscription: id,
                cursor: cursorToken(sub.cursor),
                reason:
                  !parsed || age < 0 || age > config.cursorMaxAgeMs
                    ? "cursor-expired"
                    : "reconnect",
              });
            }
            live(connection);
            return sub.handle;
          } catch (error) {
            removeSub(sub, "revoked", false);
            throw error;
          }
        },
        async snapshot(subscription, read) {
          live(connection);
          const sub = connection.subscriptions.get(subscription.id);
          if (!sub || sub.handle !== subscription || sub.initializing)
            throw new RealtimeError("denied");
          if (sub.validUntil <= Date.now()) {
            removeSub(sub, "expired");
            throw new RealtimeError("expired");
          }
          const result = await run(sub.abort.signal, async (snapshotSignal) => {
            const before = await current(sub.topic);
            const value = await read(snapshotSignal);
            snapshotSignal.throwIfAborted();
            const after = await current(sub.topic);
            return { value, after, stable: sameCursor(before, after) };
          });
          live(connection);
          if (!connection.subscriptions.has(subscription.id) || sub.validUntil <= Date.now())
            throw new RealtimeError("expired");
          if (!result.stable)
            push(connection, {
              version: 1,
              kind: "gap",
              subscription: subscription.id,
              reason: "snapshot-race",
              cursor: cursorToken(result.after),
            });
          return { value: result.value, cursor: cursorToken(result.after), stable: result.stable };
        },
        [Symbol.asyncIterator]() {
          if (connection.consumed) throw new RealtimeError("invalid-input");
          connection.consumed = true;
          return {
            next: () => next(connection),
            async return() {
              closeConnection(connection);
              return { done: true, value: undefined };
            },
          };
        },
        response() {
          const iterator = api[Symbol.asyncIterator]();
          const encoder = new TextEncoder();
          let first = true;
          const body = new ReadableStream<Uint8Array>(
            {
              async pull(controller) {
                try {
                  const result = await iterator.next();
                  if (result.done) {
                    controller.close();
                    return;
                  }
                  const event = result.value;
                  const prefix = first ? `retry: ${config.retryMs}\n` : "";
                  first = false;
                  const id = event.cursor ? `id: ${event.cursor}\n` : "";
                  controller.enqueue(
                    encoder.encode(
                      `${prefix}${id}event: ${event.kind}\ndata: ${JSON.stringify(event)}\n\n`,
                    ),
                  );
                } catch {
                  closeConnection(connection);
                  controller.error(new RealtimeError("closed"));
                }
              },
              cancel() {
                closeConnection(connection);
              },
            },
            { highWaterMark: 0 },
          );
          return new Response(body, {
            headers: {
              "content-type": "text/event-stream; charset=utf-8",
              "cache-control": "no-store",
              "x-accel-buffering": "no",
            },
          });
        },
        close() {
          closeConnection(connection);
        },
      };
      return api;
    },
    async publish(resource, type, data) {
      if (typeof type !== "string" || !/^[a-zA-Z][a-zA-Z0-9._-]{0,63}$/.test(type))
        throw new RealtimeError("invalid-input");
      return publish(resource, { kind: "update", type, data });
    },
    async revokeResource(resource) {
      const topic = resourceTopic(resource);
      for (const sub of [...(topics.get(topic) ?? [])]) removeSub(sub, "revoked");
      await publish(resource, { kind: "revoke" });
    },
    async deleteResource(resource) {
      const topic = resourceTopic(resource);
      for (const sub of [...(topics.get(topic) ?? [])]) removeSub(sub, "deleted");
      await publish(resource, { kind: "deleted" });
    },
    revokeSubject(scope, subject) {
      part(scope);
      part(subject);
      for (const connection of [...connections])
        if (connection.identity.scope === scope && connection.identity.subject === subject)
          closeConnection(connection, { version: 1, kind: "closed", reason: "revoked" });
    },
    stats() {
      return {
        connections: connections.size,
        topics: topics.size,
        pending,
        subscriptions: [...connections].reduce((sum, c) => sum + c.subscriptions.size, 0),
      };
    },
    close() {
      return (closing ??= (async () => {
        stopped = true;
        clearInterval(sweep);
        lifetime.abort(new RealtimeError("closed"));
        for (const connection of [...connections])
          closeConnection(connection, { version: 1, kind: "closed", reason: "shutdown" });
        await provider.close();
      })());
    },
  };
}
