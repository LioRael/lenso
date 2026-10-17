import { duration, name, type EventHandler, type EventStore } from "./contracts";
export type {
  Json,
  Event,
  Delivery,
  DeliveryStatus,
  EventHandler,
  EventStore,
  AsyncEventStore,
} from "./contracts";

export function createEvents(options: {
  store: EventStore;
  leaseMs?: number;
  retryDelayMs?: number;
}) {
  const { store } = options;
  const leaseMs = options.leaseMs ?? 30_000;
  const retryDelayMs = options.retryDelayMs ?? 1_000;
  duration(leaseMs);
  duration(retryDelayMs);
  const handlers = new Map<string, EventHandler>();
  const active = new Map<string, Set<AbortController>>();
  const runs = new Set<Promise<unknown>>();
  let closed = false;
  let closing: Promise<void> | undefined;
  function ready() {
    if (closed) throw new Error("Events is closed");
  }
  function abort(id: string) {
    for (const controller of active.get(id) ?? []) controller.abort();
  }

  const service = {
    publish(event: Parameters<EventStore["publish"]>[0]) {
      ready();
      return store.publish(event);
    },
    status(eventId: string) {
      ready();
      return store.status(eventId);
    },
    /** Attach to a durable subscription. Closing the service only detaches handlers. */
    subscribe(id: string, topic: string, handler: EventHandler) {
      ready();
      name(id);
      name(topic);
      if (handlers.has(id)) throw new Error("Subscription already attached");
      if (typeof handler !== "function") throw new Error("Invalid event handler");
      store.register(id, topic);
      handlers.set(id, handler);
      return () => service.unsubscribe(id);
    },
    /** Permanent cancellation; use a new ID to subscribe again. */
    unsubscribe(id: string) {
      ready();
      store.cancel(id);
      handlers.delete(id);
      abort(id);
    },
    /** One bounded pass. No timers, queue creation, or implicit task registration. */
    dispatch(limit = 100) {
      ready();
      if (!Number.isInteger(limit) || limit < 1 || limit > 1_000)
        throw new Error("Dispatch limit must be between 1 and 1000");
      const run = Promise.resolve().then(async () => {
        const result = { delivered: 0, retried: 0, lost: 0 };
        for (let i = 0; i < limit && !closed; i++) {
          // Claim only the next event, so queued handlers do not consume their leases.
          const delivery = store.claim([...handlers.keys()], leaseMs);
          if (!delivery) break;
          const controller = new AbortController();
          const controllers = active.get(delivery.subscriptionId) ?? new Set<AbortController>();
          active.set(delivery.subscriptionId, controllers);
          controllers.add(controller);
          try {
            let succeeded = false;
            try {
              await handlers.get(delivery.subscriptionId)!(delivery.event, {
                subscriptionId: delivery.subscriptionId,
                idempotencyKey: delivery.idempotencyKey,
                attempt: delivery.attempt,
                signal: controller.signal,
              });
              succeeded = true;
            } catch {
              // Do not persist arbitrary handler errors, which can contain credentials/payloads.
            }
            // Storage failures propagate. Never turn a failed acknowledgement into success.
            if (succeeded) {
              if (store.acknowledge(delivery)) result.delivered++;
              else result.lost++;
            } else if (store.retry(delivery, retryDelayMs)) result.retried++;
            else result.lost++;
          } finally {
            controllers.delete(controller);
            if (!controllers.size) active.delete(delivery.subscriptionId);
          }
        }
        return result;
      });
      runs.add(run);
      void run.then(
        () => runs.delete(run),
        () => runs.delete(run),
      );
      return run;
    },
    /** Abort cooperatively, drain active handlers, retain durable subscriptions/pending events. */
    close(): Promise<void> {
      if (closing) return closing;
      closed = true;
      for (const id of active.keys()) abort(id);
      closing = Promise.allSettled(runs).then(() => {
        handlers.clear();
      });
      return closing;
    },
  };
  return service;
}

export type Events = ReturnType<typeof createEvents>;
