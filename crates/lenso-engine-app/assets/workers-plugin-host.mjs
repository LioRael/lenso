// Generated target bridge: dependency routes come exclusively from the Kernel.
import { prepareWorkersRequestPlugin } from "./runtime/plugin.mjs";
// LENSO_JS_DEFINITIONS

function context(scope, cancelled, budget, requestId, kernelContextKey = requestId) {
  const controller = new AbortController();
  const deadline = Date.now() + budget;
  const timer = setTimeout(() => controller.abort(), budget);
  return {
    requestId,
    kernelContextKey,
    signal: controller.signal,
    get cancelled() {
      if (scope.invalidated || controller.signal.aborted || cancelled()) {
        controller.abort();
        return true;
      }
      return false;
    },
    remainingTimeoutMs: () => Math.max(0, deadline - Date.now()),
    abort: () => controller.abort(),
    close: () => clearTimeout(timer),
  };
}

export function constructPlugin(scope, options, cancelled, invoke, budget) {
  return scope.operation(() => {
    const admitted = JSON.parse(options);
    const lifecycle = context(scope, cancelled, budget, "construct");
    return { abort: () => lifecycle.abort(), promise: (async () => {
    try {
      const dependencies = Object.fromEntries(admitted.requirements.map(id => [id, []]));
      for (const binding of admitted.bindings) {
        dependencies[binding.requirement_id].push({
          providerInstance: binding.provider_instance,
          descriptor: {
            capability_id: binding.capability_id,
            descriptor_version: binding.descriptor_version,
            descriptor_digest: binding.descriptor_digest,
            operations: binding.request_operations,
            stream_operations: [], event_operations: [],
          },
          invokeRequest: async (operation, call, payload) => {
            if (scope.invalidated || call.cancelled) return { kind: "runtime", failure: { kind: "cancelled" } };
            const result = JSON.parse(await invoke(binding.binding_id, operation, JSON.stringify(payload), call));
            return "ok" in result ? { kind: "success", value: result.ok }
              : "error" in result ? { kind: "domain", value: result.error }
              : { kind: "runtime", failure: result.runtime };
          },
        });
      }
      return await prepareWorkersRequestPlugin(definitions[admitted.plugin], {
        providedEndpoints: admitted.endpoints,
        dependencies, configuration: admitted.configuration, lifecycle,
      });
    } finally { lifecycle.close(); }
    })() };
  }).promise;
}

export function invokePlugin(scope, plugin, capability, operation, payload, cancelled, budget, requestId, kernelContextKey) {
  return scope.operation(() => {
    const call = context(scope, cancelled, budget, requestId, kernelContextKey);
    return { abort: () => call.abort(), promise: (async () => {
    try {
      // Dependency callbacks use this invocation's Kernel context, never a
      // fresh JS route or the construction scope. The route closure delegates
      // through the current call token retained by the generation.
      return JSON.stringify(await plugin.invokeRequest(capability, operation, call, JSON.parse(payload)));
    } finally { call.close(); }
    })() };
  }).promise;
}

export function stopPlugin(scope, plugin, cancelled, budget) {
  // Cleanup may follow request cancellation (closed admission). Track physical
  // work without admitting another request, and fence every Wasm continuation.
  if (scope.invalidated) return new Promise(() => {});
  const pending = scope.trackNative((async () => {
    const lifecycle = context(scope, cancelled, budget, "stop");
    try { await plugin.stop(lifecycle); } finally { lifecycle.close(); }
  })());
  return new Promise((resolve, reject) => pending.then(
    value => { if (!scope.invalidated) resolve(value); },
    error => { if (!scope.invalidated) reject(error); },
  ));
}
