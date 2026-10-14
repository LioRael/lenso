import { defineApp } from "@lenso/core";
import { definePlugin } from "@lenso/core/plugin";
import { aggregate } from "./aggregate";
import type { AggregateInput } from "./generated/logs";

export const batch = definePlugin({
  id: "batch",
  setup() {
    console.error(JSON.stringify({ event: "plugin_initialized", plugin: "batch", count: 1 }));
    return {
      aggregate(input: AggregateInput, signal?: AbortSignal) {
        const started = performance.now();
        const output = aggregate(input, signal);
        const business_ms = performance.now() - started;
        if (process.env.LENSO_PERF_TIMING === "1")
          console.error(JSON.stringify({ event: "aggregate_completed", business_ms }));
        return output;
      },
    };
  },
});

export default defineApp({ plugins: [batch] });
