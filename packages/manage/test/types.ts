import { definePlugin, type PluginContext } from "@lenso/core";
import { defineOperation, type OperationBinding } from "@lenso/engine/operations";
import { z } from "zod";
import {
  bindManageOperation,
  defineManage,
  selectManageOperations,
  createManageAdapter,
  createManageSelection,
} from "../src";
import { createManageRouter } from "../src/orpc";

const input = z.object({ title: z.string() });
const plugin = definePlugin({
  id: "typed",
  setup: () => ({
    read: (value: z.infer<typeof input>, context: { credential: string }) => ({
      title: value.title,
      present: Boolean(context.credential),
    }),
  }),
});
const read = defineOperation({ plugin, method: "read", input, description: "Read", context: true });
bindManageOperation(read, { context: { credential: "fixture" } });
// @ts-expect-error Context type comes from the existing second service parameter.
bindManageOperation(read, { context: { credential: 123 } });
// @ts-expect-error A declared contextual binding requires its trusted context.
bindManageOperation(read, {});
const manage = defineManage({ plugin, operations: [read] });
selectManageOperations(manage, ["read"]);
// @ts-expect-error Selection uses preserved declared method literals.
selectManageOperations(manage, ["hidden"]);

function actualBindingTypes(running: PluginContext) {
  const options = { running, plugins: [plugin], operations: [read], canList: () => true };
  createManageAdapter({ ...options, binding: () => ({ context: { credential: "trusted" } }) });
  // @ts-expect-error The real adapter binding cannot erase the service context type.
  createManageAdapter({ ...options, binding: () => ({ context: { credential: 123 } }) });
  // @ts-expect-error Contextual declarations require a context in the real entry binding.
  createManageAdapter({ ...options, binding: () => ({}) });
  const selection = createManageSelection(options);
  selection.createAdapter({
    canList: () => true,
    binding: () => ({ context: { credential: "trusted" } }),
  });
  selection.createAdapter({
    canList: () => true,
    // @ts-expect-error Prepared selections retain their operation context contract.
    binding: () => ({ context: { credential: 123 } }),
  });
  createManageAdapter({
    selection,
    canList: () => true,
    // @ts-expect-error Selection-based adapters cannot erase contextual binding requirements.
    binding: () => ({}),
  });
  createManageRouter({
    ...options,
    evidence: () => ({ evidence: "trusted" }),
    binding: (_operation, _input, evidence) => ({ context: { credential: evidence.evidence } }),
  });
  createManageRouter({
    ...options,
    evidence: () => ({ evidence: "trusted" }),
    // @ts-expect-error The request adapter also checks the actual service context.
    binding: () => ({ context: { credential: 123 } }),
  });
  createManageRouter({
    selection,
    evidence: () => ({ evidence: "trusted" }),
    canList: () => true,
    binding: (_operation, _input, evidence) => ({ context: { credential: evidence.evidence } }),
  });
  createManageRouter({
    selection,
    evidence: () => ({ evidence: "trusted" }),
    canList: () => true,
    // @ts-expect-error Selection-based routers preserve the actual context type.
    binding: () => ({ context: { credential: 123 } }),
  });
  const launch: OperationBinding<typeof read> = (_operation, _input, app) => {
    void app.stop;
    void app.status;
    return { context: { credential: "trusted" } };
  };
  // @ts-expect-error Typed CLI launch bindings cannot supply an unrelated context.
  const invalidLaunch: OperationBinding<typeof read> = () => ({ context: { actor: "forged" } });
  void launch;
  void invalidLaunch;
}
void actualBindingTypes;
