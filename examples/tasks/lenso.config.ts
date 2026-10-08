import { defineApp } from "@lenso/core";
import { selectManageOperations } from "@lenso/manage";
import { taskOperations, tasks } from "./src/plugin";

export default defineApp({ plugins: [tasks] });
export const manage = [taskOperations.manage];
export const operations = [
  ...selectManageOperations(taskOperations.manage, ["submit", "query", "cancel", "retry"]),
  taskOperations.operations[4]!,
];
export const mcpOperations = selectManageOperations(taskOperations.manage, [
  "submit",
  "query",
  "cancel",
  "retry",
]);
