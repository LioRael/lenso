import { defineApp } from "@lenso/core";
import { defineOperation } from "@lenso/cli";
import { local, logs, logsInput, runtime } from "./src/plugins";

export default defineApp({ plugins: [runtime, local, logs] });
export const operations = [
  defineOperation({
    plugin: logs,
    method: "aggregate",
    input: logsInput,
    description: "Aggregate synthetic logs in the Rust batch service and record a local batch.",
    effect: "write",
    retry: "unsafe",
    source: { file: "src/plugins.ts", export: "logs" },
  }),
];
// Streams are consumed by application code, not finite CLI/Manage JSON operations.
export const mcpOperations = [];
