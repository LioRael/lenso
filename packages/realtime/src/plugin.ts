import { bindConfig, definePluginConfig, type ConfigSource } from "@lenso/core/config";
import type { Plugin } from "@lenso/core/plugin";
import { defaults, resolveRealtimeConfig } from "./config";
import { createRealtime } from "./index";
import type { Realtime, RealtimeConfig, RealtimeOptions, RealtimeProvider } from "./contracts";

const schema = {
  "~standard": {
    version: 1 as const,
    vendor: "lenso-realtime",
    types: undefined as { input: RealtimeConfig; output: Required<RealtimeConfig> } | undefined,
    validate(input: unknown) {
      try {
        if (!input || typeof input !== "object" || Array.isArray(input))
          return { issues: [{ message: "Invalid realtime configuration" }] };
        return { value: resolveRealtimeConfig(input as RealtimeConfig) };
      } catch {
        return { issues: [{ message: "Invalid realtime configuration" }] };
      }
    },
  },
};

export const realtimeConfig = definePluginConfig({
  schema,
  description: "Bounded resource subscriptions; no stream or publisher is automatically exposed.",
  fields: Object.keys(defaults).map((name) => ({
    path: [name],
    description: `Realtime ${name} limit.`,
  })),
  jsonSchema: () => ({
    type: "object",
    additionalProperties: false,
    properties: Object.fromEntries(
      Object.keys(defaults).map((key) => [
        key,
        {
          type: "integer",
          minimum: key === "maxBufferedBytes" ? 512 : 1,
          maximum:
            key === "authorizationLeaseMs"
              ? 30000
              : key === "sweepMs"
                ? 1000
                : key === "maxPayloadBytes"
                  ? 32768
                  : 10000000,
        },
      ]),
    ),
  }),
});

/** Provider factory executes in setup, never at trusted config import time. */
export function createRealtimePlugin<P>(options: {
  id: string;
  requires?: readonly Plugin<unknown>[];
  config: RealtimeConfig | readonly ConfigSource[];
  provider(): RealtimeProvider;
  authorize: RealtimeOptions<P>["authorize"];
}): Plugin<Realtime<P>> {
  return bindConfig(realtimeConfig, options.config, {
    id: options.id,
    requires: options.requires,
    async setup(context, config) {
      const provider = options.provider();
      context.onCleanup(() => provider.close());
      const realtime = await createRealtime<P>({
        config,
        provider,
        authorize: options.authorize,
        onDiagnostic(event) {
          context.logger?.warn({ category: event }, "Realtime lifecycle diagnostic");
        },
      });
      context.onCleanup(() => realtime.close());
      return realtime;
    },
  });
}
