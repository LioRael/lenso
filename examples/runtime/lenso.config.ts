import { defineApp } from "@lenso/core";
import { defineOperation } from "@lenso/cli";
import { RuntimeError } from "@lenso/runtime-sdk";
import { local, logs, logsInput, runtime } from "./src/plugins";

const runtimeMessages: Readonly<Record<string, string>> = {
  service_not_found: "The selected Runtime service is unavailable.",
  not_ready: "The selected Runtime service is not ready.",
  overloaded: "Runtime rejected the call at its bounded admission limit.",
  deadline_exceeded: "The Runtime call exceeded its deadline.",
  connection_lost: "The Runtime connection was lost; the call was not replayed.",
  cancelled: "The Runtime call was cancelled; this does not prove side effects were undone.",
  contract_mismatch: "The Runtime service contract is incompatible with this client.",
  invalid_input: "Runtime rejected input at the explicit service boundary.",
  invalid_output: "Runtime rejected the service response at the explicit boundary.",
  business: "The Runtime service returned a declared business error.",
  protocol_error: "The Runtime connection violated the local protocol.",
};

export default defineApp({ plugins: [runtime, local, logs] });
export const operations = [
  defineOperation({
    plugin: logs,
    method: "aggregate",
    input: logsInput,
    description:
      "Aggregate synthetic logs in the selected Runtime batch service and record a local batch.",
    effect: "write",
    retry: "unsafe",
    mapError(error) {
      if (error instanceof RuntimeError && Object.hasOwn(runtimeMessages, error.code))
        return { code: error.code, phase: "invoke", message: runtimeMessages[error.code]! };
    },
    source: { file: "src/plugins.ts", export: "logs" },
  }),
];
// Streams are consumed by application code, not finite CLI/Manage JSON operations.
export const mcpOperations = [];
