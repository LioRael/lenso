import {
  duration,
  name,
  type AsyncEventStore,
  type EventHandler,
  type EventStore,
} from "./contracts";
export type { AsyncEventStore } from "./contracts";

/** Async store entry; the existing synchronous service and SQLite API stay unchanged. */
export function createAsyncEvents(options: {
  store: AsyncEventStore | EventStore;
  leaseMs?: number;
  retryDelayMs?: number;
}) {
  const { store } = options;
  const leaseMs = options.leaseMs ?? 30_000;
  const retryDelayMs = options.retryDelayMs ?? 1_000;
  duration(leaseMs);
  duration(retryDelayMs);
  const handlers = new Map<string, EventHandler>();
  const attached = new Set<string>();
  const mutations = new Map<string, Promise<unknown>>();
  const active = new Map<string, Set<AbortController>>();
  const operations = new Set<Promise<unknown>>();
  let closed = false;
  let closing: Promise<void> | undefined;

  function track<T>(run: () => Promise<T>): Promise<T> {
    if (closed) return Promise.reject(new Error("Events is closed"));
    const operation = Promise.resolve().then(run);
    operations.add(operation);
    void operation.then(
      () => operations.delete(operation),
      () => operations.delete(operation),
    );
    return operation;
  }
  function mutate<T>(id: string, run: () => Promise<T>): Promise<T> {
    const previous = mutations.get(id) ?? Promise.resolve();
    const operation = previous.catch(() => {}).then(run);
    mutations.set(id, operation);
    const cleanup = () => {
      if (mutations.get(id) === operation) mutations.delete(id);
    };
    void operation.then(cleanup, cleanup);
    return operation;
  }
  function abort(id: string) {
    for (const controller of active.get(id) ?? []) controller.abort();
  }

  const service = {
    publish(event: Parameters<EventStore["publish"]>[0]) {
      return track(async () => store.publish(event));
    },
    status(eventId: string) {
      return track(async () => store.status(eventId));
    },
    subscribe(id: string, topic: string, handler: EventHandler): Promise<() => Promise<void>> {
      return track(async () => {
        name(id);
        name(topic);
        if (attached.has(id)) throw new Error("Subscription already attached");
        if (typeof handler !== "function") throw new Error("Invalid event handler");
        attached.add(id);
        return mutate(id, async () => {
          try {
            await store.register(id, topic);
            if (!closed) handlers.set(id, handler);
            return () => service.unsubscribe(id);
          } catch (error) {
            attached.delete(id);
            throw error;
          }
        });
      });
    },
    unsubscribe(id: string): Promise<void> {
      return track(async () => {
        name(id);
        return mutate(id, async () => {
          await store.cancel(id);
          handlers.delete(id);
          attached.delete(id);
          abort(id);
        });
      });
    },
    dispatch(limit = 100) {
      return track(async () => {
        if (!Number.isInteger(limit) || limit < 1 || limit > 1_000)
          throw new Error("Dispatch limit must be between 1 and 1000");
        const result = { delivered: 0, retried: 0, lost: 0 };
        for (let i = 0; i < limit && !closed; i++) {
          const delivery = await store.claim([...handlers.keys()], leaseMs);
          if (!delivery) break;
          const handler = handlers.get(delivery.subscriptionId);
          // close/cancel may finish while claim is in flight. Do not start a detached
          // handler; an unconfirmed claim remains recoverable by lease expiry.
          if (closed || !handler) {
            result.lost++;
            continue;
          }
          const controller = new AbortController();
          const controllers = active.get(delivery.subscriptionId) ?? new Set<AbortController>();
          active.set(delivery.subscriptionId, controllers);
          controllers.add(controller);
          try {
            let succeeded = false;
            try {
              await handler(delivery.event, {
                subscriptionId: delivery.subscriptionId,
                idempotencyKey: delivery.idempotencyKey,
                attempt: delivery.attempt,
                signal: controller.signal,
              });
              succeeded = true;
            } catch {
              // Arbitrary handler errors can contain credentials or payloads.
            }
            if (succeeded) {
              if (await store.acknowledge(delivery)) result.delivered++;
              else result.lost++;
            } else if (await store.retry(delivery, retryDelayMs)) result.retried++;
            else result.lost++;
          } finally {
            controllers.delete(controller);
            if (!controllers.size) active.delete(delivery.subscriptionId);
          }
        }
        return result;
      });
    },
    /** Drains accepted operations as well as handlers. The borrowed store stays open. */
    close(): Promise<void> {
      if (closing) return closing;
      closed = true;
      for (const id of active.keys()) abort(id);
      closing = Promise.allSettled(operations).then(() => {
        handlers.clear();
        attached.clear();
      });
      return closing;
    },
  };
  return service;
}

export type AsyncEvents = ReturnType<typeof createAsyncEvents>;
