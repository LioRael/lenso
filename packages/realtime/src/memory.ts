import {
  RealtimeError,
  type Cursor,
  type Delivery,
  type ProviderEvent,
  type RealtimeProvider,
} from "./contracts";
import { serialize } from "./wire";

export function createMemoryProvider(
  options: { maxTopics?: number; watermarkTtlMs?: number } = {},
): RealtimeProvider {
  const maxTopics = options.maxTopics ?? 10000;
  const ttl = options.watermarkTtlMs ?? 600000;
  if (!Number.isSafeInteger(maxTopics) || maxTopics < 1 || !Number.isSafeInteger(ttl) || ttl < 1)
    throw new RealtimeError("invalid-input");
  const marks = new Map<string, { cursor: Cursor; expiresAt: number }>();
  let deliver: ((delivery: Delivery) => void) | undefined;
  let closed = false;
  const current = (topic: string): Cursor => {
    if (!deliver || closed) throw new RealtimeError("closed");
    const now = Date.now();
    const old = marks.get(topic);
    if (old && old.expiresAt > now) {
      old.expiresAt = now + ttl;
      return { ...old.cursor };
    }
    if (marks.size >= maxTopics) {
      for (const [key, mark] of marks) if (mark.expiresAt <= now) marks.delete(key);
      if (marks.size >= maxTopics) throw new RealtimeError("limit");
    }
    const cursor = { generation: crypto.randomUUID(), sequence: 0 };
    marks.set(topic, { cursor, expiresAt: now + ttl });
    return { ...cursor };
  };
  return {
    kind: "memory",
    async start(listener) {
      if (deliver || closed) throw new RealtimeError("closed");
      deliver = listener;
    },
    async current(topic) {
      return current(topic);
    },
    async publish(topic: string, event: ProviderEvent) {
      const copy = JSON.parse(serialize(event, 49152)) as ProviderEvent;
      const previous = current(topic);
      const cursor = { ...previous, sequence: previous.sequence + 1 };
      marks.set(topic, { cursor, expiresAt: Date.now() + ttl });
      deliver!({ topic, event: copy, cursor });
      return { ...cursor };
    },
    async close() {
      closed = true;
      deliver = undefined;
      marks.clear();
    },
  };
}
