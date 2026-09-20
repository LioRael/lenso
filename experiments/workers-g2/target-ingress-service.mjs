// The target Worker owns a real fetch handler for the local-workerd cohort.
// It intentionally contains no qualification assertions: the separate test
// Worker reaches this handler through a Workerd service binding.
import * as bindings from "./pkg/lenso_workers_g2_host.js";
import module from "./pkg/lenso_workers_g2_host_bg.wasm";
import { clearTimers } from "./clock.mjs";
import { createHttpHandler } from "../../packages/workers-runtime/http.mjs";
import { createEventRunner } from "../../packages/workers-runtime/runner.mjs";

const runner = createEventRunner({
  instantiate: () => bindings.initSync({ module }),
  resetState: bindings.__wbg_reset_state,
  clearTimers,
  eventLimitMs: 100,
  cancellationLimitMs: 50,
});

const handler = createHttpHandler({
  run: runner.run,
  handleHttp: bindings.handle_http,
  maxRequestBodyBytes: 65536,
  maxResponseBodyBytes: 65536,
  maxRequestHeadBytes: 16384,
  bodyReadTimeoutMs: 5,
  onReceipt(result, response) {
    response.headers.set("x-g2-shutdown", result.shutdown);
  },
});

export default {
  fetch(request) {
    return handler(request);
  },
};
