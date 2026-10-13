import { definePlugin, type Plugin } from "@lenso/core/plugin";
import { defineOperation } from "@lenso/engine/operations";
import { defineManage } from "@lenso/manage";
import type { StandardJSONSchemaV1, StandardSchemaV1 } from "@standard-schema/spec";
import type { RuntimeService } from "./index";

const emptyObject = () => ({ type: "object", properties: {}, additionalProperties: false });
const statusInput: StandardSchemaV1<Record<string, never>, Record<string, never>> &
  StandardJSONSchemaV1<Record<string, never>, Record<string, never>> = {
  "~standard": {
    version: 1,
    vendor: "@lenso/runtime",
    jsonSchema: { input: emptyObject, output: emptyObject },
    validate(value) {
      return value !== null &&
        typeof value === "object" &&
        !Array.isArray(value) &&
        Object.keys(value).length === 0
        ? { value: value as Record<string, never> }
        : { issues: [{ message: "Expected an empty object." }] };
    },
  },
};

/** The host supplies its existing Auth/evidence policy, never a JSON principal. */
export function createRuntimeManage<Clients, Context>(options: {
  id: string;
  runtime: Plugin<RuntimeService<Clients>>;
  authorize(context: Context): void | Promise<void>;
}) {
  const plugin = definePlugin({
    id: options.id,
    requires: [options.runtime],
    setup(context) {
      const { host } = context.get(options.runtime);
      return {
        async status(_input: Record<string, never>, trusted: { readonly evidence: Context }) {
          await options.authorize(trusted.evidence);
          return host.status();
        },
      };
    },
  });
  const status = defineOperation({
    plugin,
    method: "status",
    input: statusInput,
    context: true,
    description: "Read the local runtime registry and active call diagnostics.",
    effect: "read",
    destructive: false,
    retry: "safe",
    cancellation: "none",
  });
  const manage = defineManage({ plugin, operations: [status] });
  return { plugin, status, manage };
}
