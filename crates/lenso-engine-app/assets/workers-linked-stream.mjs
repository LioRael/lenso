// Generated Host assembly. Routing, protocols and business rules stay in Plugins.
import * as bindings from "./host.js";
import wasmModule from "./host_bg.wasm";
import { clearTimers } from "./runtime/clock.mjs";
import { createStreamingHttpHandler } from "./runtime/http.mjs";
import { createEventRunner } from "./runtime/runner.mjs";
import { createEventScope } from "./runtime/scope.mjs";
// LENSO_SCOPE
const limits = /* LENSO_LIMITS */ {};
const pick = (names) => Object.fromEntries(names.filter((name) => name in limits).map((name) => [name, limits[name]]));
const runner = createEventRunner({
  instantiate() {
    const exports = bindings.initSync({ module: wasmModule });
    if (typeof exports.__wasm_call_ctors === "function") return exports;
    return { ...exports, __wasm_call_ctors: bindings.__wasm_call_ctors };
  },
  resetState: bindings.__wbg_reset_state,
  clearTimers,
  ...pick(["eventLimitMs", "maxConcurrent", "retirementAdmissionLimit", "sessionLimitMs", "cancellationLimitMs"]),
});
const chunkBytes = limits.maxResponseChunkBytes ?? 65536;
export default {
  fetch(request, env, ctx) {
    const handler = createStreamingHttpHandler({
      async open(operation, options) {
        const session = await runner.open(operation, options);
        // Response-body errors and disconnects can end the transport context
        // before its request App and native facilities finish cleanup.
        ctx?.waitUntil?.(session.closed.catch(() => {}));
        return session;
      },
      ...pick(["maxRequestBodyBytes", "maxRequestHeadBytes", "maxResponseBodyBytes", "maxResponseChunkBytes", "bodyReadTimeoutMs"]),
      createScope: () => typeof createScope === "function"
        ? createScope(request, env, ctx)
        : createEventScope({}, pick(["cleanupTimeoutMs", "maxOperations"])),
      async openHttp(input, scope) {
        const response = await bindings.open_http(input, scope);
        // Abandonment resets wasm-bindgen's global namespace. Never free a
        // previous generation's pointer against the replacement memory.
        if (scope.invalidated) throw new Error("session_generation_retired");
        const closed = response.closed.finally(() => {
          if (!scope.invalidated) response.free();
        });
        // Own rejection even if constructing the transport head fails.
        closed.catch(() => {});
        return {
          value: { status: response.status, headers: JSON.parse(response.headers), read: () => response.read(chunkBytes) },
          closed,
        };
      },
    });
    return handler(request);
  },
};
