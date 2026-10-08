import { defineApp } from "@lenso/core";
import { defineOperation } from "@lenso/cli";
import { jobInput, reportQueryInput, submitInput } from "./src/contracts";
import { tasks } from "./src/plugin";

export default defineApp({ plugins: [tasks] });
export const operations = [
  defineOperation({
    plugin: tasks,
    method: "submit",
    input: submitInput,
    description: "Submit a report owned by the authenticated session subject.",
    effect: "write",
    destructive: false,
    retry: "unsafe",
    cancellation: "none",
    outputDescription:
      "An owned jobId. Without a deduplication key, repeating creates another job.",
    source: { file: "src/plugin.ts", export: "tasks" },
  }),
  defineOperation({
    plugin: tasks,
    method: "query",
    input: jobInput,
    description: "Read safe status after durable owner authorization.",
    effect: "read",
    destructive: false,
    retry: "safe",
    cancellation: "none",
    outputDescription:
      "State, attempt budget and cancellation request flag, or null after pruning.",
    source: { file: "src/plugin.ts", export: "tasks" },
  }),
  defineOperation({
    plugin: tasks,
    method: "cancel",
    input: jobInput,
    description: "Request cancellation of an owned job; requested does not mean stopped.",
    effect: "write",
    destructive: true,
    retry: "safe",
    cancellation: "request-only",
    outputDescription:
      "cancelled, requested, terminal or missing. No external effects are rolled back.",
    source: { file: "src/plugin.ts", export: "tasks" },
  }),
  defineOperation({
    plugin: tasks,
    method: "retry",
    input: jobInput,
    description: "Retry an owned final failure, preserving payload and attempt count.",
    effect: "write",
    destructive: false,
    retry: "unsafe",
    cancellation: "none",
    outputDescription: "true only if a final failure received one more attempt; otherwise false.",
    source: { file: "src/plugin.ts", export: "tasks" },
  }),
  defineOperation({
    plugin: tasks,
    method: "report",
    input: reportQueryInput,
    description: "Read an owned report from the business table.",
    effect: "read",
    destructive: false,
    retry: "safe",
    cancellation: "none",
    outputDescription: "sum and count, or null before the report is written.",
    source: { file: "src/plugin.ts", export: "tasks" },
  }),
];
