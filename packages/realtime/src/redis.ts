// RedisClient exposes callback properties, not EventTarget listeners.
/* oxlint-disable unicorn/prefer-add-event-listener */
import { RedisClient } from "bun";
import {
  RealtimeError,
  type Cursor,
  type Delivery,
  type ProviderEvent,
  type RealtimeProvider,
} from "./contracts";
import { serialize } from "./wire";

export interface RedisProviderOptions {
  readonly url: string;
  readonly namespace: string;
  readonly watermarkTtlMs?: number;
  readonly commandTimeoutMs?: number;
  readonly connectionTimeoutMs?: number;
  readonly maxPendingOperations?: number;
}

const MAX_WIRE_BYTES = 48 * 1024;
const encoder = new TextEncoder();

// Watermarks are metadata, not an event log. Expiry starts a new generation.
const WATERMARK = `
if redis.call('EXISTS', KEYS[1]) == 0 then
  redis.call('HSET', KEYS[1], 'generation', ARGV[1], 'sequence', '0')
end
local generation = redis.call('HGET', KEYS[1], 'generation')
local sequence = redis.call('HGET', KEYS[1], 'sequence')
if ARGV[3] == 'publish' then
  if tonumber(sequence) >= 9007199254740991 then
    return redis.error_reply('sequence exhausted')
  end
  redis.call('HINCRBY', KEYS[1], 'sequence', 1)
  sequence = redis.call('HGET', KEYS[1], 'sequence')
  local delivery = '{"topic":' .. ARGV[4] .. ',"cursor":{"generation":"' ..
    generation .. '","sequence":' .. sequence .. '},"event":' .. ARGV[5] .. '}'
  redis.call('PUBLISH', ARGV[6], delivery)
end
redis.call('PEXPIRE', KEYS[1], ARGV[2])
return {generation, sequence}
`;

function validCursor(value: unknown): value is Cursor {
  if (!value || typeof value !== "object") return false;
  const cursor = value as Cursor;
  return (
    Object.keys(cursor).length === 2 &&
    typeof cursor.generation === "string" &&
    /^[A-Za-z0-9-]{1,64}$/.test(cursor.generation) &&
    Number.isSafeInteger(cursor.sequence) &&
    cursor.sequence >= 0
  );
}

function validEvent(value: unknown): value is ProviderEvent {
  if (!value || typeof value !== "object") return false;
  const event = value as ProviderEvent;
  if (event.kind === "deleted" || event.kind === "revoke") return Object.keys(event).length === 1;
  return (
    event.kind === "update" &&
    typeof event.type === "string" &&
    event.type.length > 0 &&
    event.type.length <= 256 &&
    Object.hasOwn(event, "data") &&
    Object.keys(event).length === 3
  );
}

function validTopic(topic: unknown): topic is string {
  return typeof topic === "string" && topic.length > 0 && topic.length <= 4096;
}

/** Standalone Redis only: two owned sockets, non-durable namespace Pub/Sub. */
export function createRedisProvider(options: RedisProviderOptions): RealtimeProvider {
  if (
    !options ||
    typeof options.url !== "string" ||
    !options.url ||
    typeof options.namespace !== "string" ||
    !/^[A-Za-z0-9:_-]{1,128}$/.test(options.namespace)
  )
    throw new RealtimeError("invalid-input");
  const ttl = options.watermarkTtlMs ?? 600000;
  const timeout = options.commandTimeoutMs ?? 5000;
  const connectionTimeout = options.connectionTimeoutMs ?? 2000;
  const maxPending = options.maxPendingOperations ?? 64;
  for (const value of [ttl, timeout, connectionTimeout, maxPending]) {
    if (!Number.isSafeInteger(value) || value < 1 || value > 10000000)
      throw new RealtimeError("invalid-input");
  }
  const url = options.url;
  const namespace = options.namespace;
  const channel = `${namespace}:events`;
  // Length framing keeps namespaces containing ":" disjoint from topic text.
  const keyPrefix = `${namespace.length}:${namespace}:watermark:`;
  let state: "new" | "starting" | "ready" | "failed" | "closed" = "new";
  let command: RedisClient | undefined;
  let subscription: RedisClient | undefined;
  let deliver: ((delivery: Delivery) => void) | undefined;
  let fail: (() => void) | undefined;
  let pending = 0;
  const cancellations = new Set<(error: RealtimeError) => void>();

  function release(error: RealtimeError): void {
    deliver = undefined;
    fail = undefined;
    for (const cancel of cancellations) cancel(error);
    for (const client of [command, subscription]) {
      if (!client) continue;
      // Bun 1.4.2 invokes onclose during close even when the typed setter accepts null.
      client.onclose = () => {};
      client.onconnect = () => {};
      // Closing the owned subscription socket also drops all its listeners.
      try {
        client.close();
      } catch {
        /* Already disconnected. */
      }
    }
    command = undefined;
    subscription = undefined;
  }

  function terminate(): void {
    if (state === "failed" || state === "closed") return;
    const notify = fail;
    state = "failed";
    release(new RealtimeError("provider"));
    notify?.();
  }

  function deadline<T>(run: () => Promise<T>): Promise<T> {
    return new Promise<T>((resolve, reject) => {
      let settled = false;
      const finish = (error?: RealtimeError, value?: T): void => {
        if (settled) return;
        settled = true;
        clearTimeout(timer);
        cancellations.delete(cancel);
        if (error) reject(error);
        else resolve(value as T);
      };
      const cancel = (error: RealtimeError): void => finish(error);
      const timer = setTimeout(() => terminate(), timeout);
      cancellations.add(cancel);
      try {
        run().then(
          (value) => finish(undefined, value),
          () => {
            terminate();
            finish(new RealtimeError("provider"));
          },
        );
      } catch {
        terminate();
        finish(new RealtimeError("provider"));
      }
    });
  }

  function receive(message: string): void {
    if (state !== "starting" && state !== "ready") return;
    try {
      if (encoder.encode(message).byteLength > MAX_WIRE_BYTES) throw new Error();
      const value = JSON.parse(message) as Delivery;
      if (
        !value ||
        typeof value !== "object" ||
        Object.keys(value).length !== 3 ||
        !validTopic(value.topic) ||
        !validCursor(value.cursor) ||
        !validEvent(value.event) ||
        value.cursor.sequence < 1
      )
        throw new Error();
      // The envelope adds nodes to the already bounded event. Validate the
      // event's structure using the same budget as outbound publication.
      serialize(value.event, MAX_WIRE_BYTES);
      deliver?.(value);
    } catch {
      terminate();
    }
  }

  async function watermark(topic: string, event?: ProviderEvent): Promise<Cursor> {
    if (state !== "ready") throw new RealtimeError(state === "failed" ? "provider" : "closed");
    if (pending >= maxPending) throw new RealtimeError("limit");
    if (!validTopic(topic)) throw new RealtimeError("invalid-input");
    const generation = crypto.randomUUID();
    const topicJson = serialize(topic, MAX_WIRE_BYTES);
    let eventJson = "";
    if (event !== undefined) {
      if (!validEvent(event)) throw new RealtimeError("payload");
      eventJson = serialize(event, MAX_WIRE_BYTES);
      // Reserve bytes, not a second structural budget, for provider metadata.
      const wire = JSON.stringify({
        topic,
        cursor: { generation, sequence: Number.MAX_SAFE_INTEGER },
        event: JSON.parse(eventJson),
      });
      if (encoder.encode(wire).byteLength > MAX_WIRE_BYTES) throw new RealtimeError("payload");
    }
    pending++;
    try {
      const result: unknown = await deadline(() =>
        command!.send("EVAL", [
          WATERMARK,
          "1",
          `${keyPrefix}${topic}`,
          generation,
          String(ttl),
          event === undefined ? "current" : "publish",
          topicJson,
          eventJson,
          channel,
        ]),
      );
      if (
        !Array.isArray(result) ||
        result.length !== 2 ||
        typeof result[1] !== "string" ||
        !/^\d+$/.test(result[1])
      ) {
        terminate();
        throw new RealtimeError("provider");
      }
      const cursor = { generation: result[0], sequence: Number(result[1]) };
      if (!validCursor(cursor)) {
        terminate();
        throw new RealtimeError("provider");
      }
      return cursor;
    } finally {
      pending--;
    }
  }

  return {
    kind: "redis",
    async start(onDelivery, onFailure) {
      if (state !== "new") throw new RealtimeError(state === "failed" ? "provider" : "closed");
      state = "starting";
      deliver = onDelivery;
      fail = onFailure;
      try {
        const clientOptions = {
          connectionTimeout,
          autoReconnect: false,
          maxRetries: 0,
          enableOfflineQueue: false,
          enableAutoPipelining: false,
        };
        command = new RedisClient(url, clientOptions);
        subscription = new RedisClient(url, clientOptions);
        command.onclose = terminate;
        subscription.onclose = terminate;
        await deadline(() => Promise.all([command!.connect(), subscription!.connect()]));
        await deadline(() => subscription!.subscribe(channel, receive));
        if (state !== "starting") throw new RealtimeError("provider");
        state = "ready";
      } catch {
        terminate();
        throw new RealtimeError("provider");
      }
    },
    current: (topic) => watermark(topic),
    publish: (topic, event) => watermark(topic, event),
    async close() {
      if (state === "closed") return;
      state = "closed";
      release(new RealtimeError("closed"));
    },
  };
}
