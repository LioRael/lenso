import type { Plugin } from "@lenso/core/plugin";
import type { AsyncEventStore, EventStore } from "./contracts";
import { createAsyncEvents, type AsyncEvents } from "./async";

/** Provider owns the connection and explicit migration, just as for synchronous Events. */
export function createAsyncEventsPlugin(options: {
  id: string;
  store: Plugin<AsyncEventStore | EventStore>;
  leaseMs?: number;
  retryDelayMs?: number;
}): Plugin<AsyncEvents> {
  return {
    id: options.id,
    requires: [options.store],
    setup(context) {
      const service = createAsyncEvents({ ...options, store: context.get(options.store) });
      context.onCleanup(() => service.close());
      return service;
    },
  };
}
