import { bindConfig, definePluginConfig } from "@lenso/core";
import { envSource } from "@lenso/core/config/env";
import type { StandardSchemaV1 } from "@standard-schema/spec";
import { createWorkerHandler } from "../../src/index";

const schema: StandardSchemaV1<{ enabled: boolean }, { enabled: boolean }> = {
  "~standard": {
    version: 1,
    vendor: "test",
    validate(value) {
      const enabled = (value as { enabled?: unknown }).enabled;
      return typeof enabled === "boolean"
        ? { value: { enabled } }
        : { issues: [{ path: ["enabled"], message: "Expected boolean" }] };
    },
  },
};
const contract = definePluginConfig({ schema });

export default createWorkerHandler<{ ENABLED?: string }>((bindings) => {
  const web = bindConfig(
    contract,
    [
      envSource({
        id: "worker-env",
        read: (name) => (name === "ENABLED" ? bindings.ENABLED : undefined),
        bindings: { enabled: { name: "ENABLED", type: "boolean" } },
      }),
    ],
    {
      id: "web",
      setup: (_context, config) => ({
        async fetch() {
          return new Response(String(config.enabled));
        },
      }),
    },
  );
  return { plugins: [web], web };
});
