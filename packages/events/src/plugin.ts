import type { Plugin } from "@lenso/core/plugin";
import { createEvents, type Events, type EventStore } from "./index";

/** Store provider owns its connection and explicit migrations. No resources at declaration time. */
export function createEventsPlugin(options: {
  id: string;
  store: Plugin<EventStore>;
  leaseMs?: number;
  retryDelayMs?: number;
}): Plugin<Events> {
  return {
    id: options.id,
    requires: [options.store],
    setup(context) {
      const service = createEvents({ ...options, store: context.get(options.store) });
      context.onCleanup(() => service.close());
      return service;
    },
  };
}
