// Target-qualification assertions deliberately reuse the generated G2 Host,
// the shared event scope and the real workerd service-binding runtime. They do
// not introduce an HTTP router or emulate a database provider.
import assert from "node:assert/strict";
import { createScopedHostCallback } from "../../packages/workers-runtime/callback.mjs";
import { createHttpHandler } from "../../packages/workers-runtime/http.mjs";
import { createEventRunner } from "../../packages/workers-runtime/runner.mjs";
import { createEventScope } from "../../packages/workers-runtime/scope.mjs";

const deferred = () => {
  let resolve, reject;
  const promise = new Promise((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
};
const turn = () => new Promise((resolve) => setTimeout(resolve, 0));
const wire = (path) =>
  JSON.stringify({ method: "GET", uri: path, headers: [], body: [] });

async function eventually(operation, deadlineMs = 250) {
  const deadline = Date.now() + deadlineMs;
  let last;
  while (Date.now() < deadline) {
    try {
      return await operation();
    } catch (error) {
      last = error;
      if (!/unavailable/.test(String(error))) throw error;
      await turn();
    }
  }
  throw last ?? Error("recovery deadline exceeded");
}

/**
 * Register target-local cases. `test` is intentionally a tiny adapter so this
 * exact suite runs under `workerd test` rather than a second router or mock
 * runtime. The callback request/response stay opaque to Runtime; Auth owns its
 * private JSON transport and the Host owns only lifecycle mechanics.
 */
export function targetQualification(test, {
  bindings,
  module,
  clearTimers,
  callbackService,
}) {
  const runner = createEventRunner({
    instantiate: () => bindings.initSync({ module }),
    resetState: bindings.__wbg_reset_state,
    clearTimers,
    eventLimitMs: 100,
    cancellationLimitMs: 50,
  });

  test("wasm-trap-generation-abandonment-late-cleanup", async () => {
    const scope = createEventScope({}, { cleanupTimeoutMs: 5 });
    const late = deferred();
    let deliveries = 0;
    const callback = createScopedHostCallback(
      scope,
      () => late.promise,
      { timeoutMs: 100 },
    );
    // The Host callback is active before the generated Wasm trap. Its native
    // completion remains owner-scoped even after the Runner fences this event.
    callback('{"operation":"consume"}').then(
      () => deliveries++,
      () => deliveries++,
    );
    const before = runner.generation();
    await assert.rejects(
      runner.run(() => bindings.trap_probe(), { scope }),
      /instance_abandoned|storage_cleanup_unconfirmed/,
    );
    assert.ok(runner.generation() > before, "trap must abandon the generated instance");
    await assert.rejects(
      runner.run(() => bindings.handle_http(wire("/method"), createEventScope())),
      /unavailable/,
    );
    late.resolve('{"outcome":"consumed"}');
    await turn();
    assert.equal(deliveries, 0, "late Host callback must not reach obsolete Wasm");
    const recovered = await eventually(() =>
      runner.run(() => bindings.handle_http(wire("/method"), createEventScope())),
    );
    assert.equal(recovered.status, 200);
    assert.equal(recovered.shutdown, "clean");
  });

  test("actual-generated-ingress-body-timeout-and-recovery", async () => {
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
    let timer;
    const body = new ReadableStream({
      start(controller) {
        controller.enqueue(new Uint8Array([97]));
        timer = setTimeout(() => {
          controller.enqueue(new Uint8Array([98]));
          controller.close();
        }, 50);
      },
      cancel() {
        clearTimeout(timer);
      },
    });
    const timeout = await handler(
      new Request("https://target.invalid/bytes", {
        method: "POST",
        body,
        duplex: "half",
      }),
    );
    assert.equal(timeout.status, 408);
    assert.deepEqual(await timeout.json(), { error: "request_body_timeout" });
    const healthy = await handler(new Request("https://target.invalid/method"));
    assert.equal(healthy.status, 200);
    assert.equal(healthy.headers.get("x-g2-shutdown"), "clean");
  });

  test("workerd-service-host-callback-failure-is-opaque-to-auth", async () => {
    const scope = createEventScope();
    const request = '{"operation":"revoke","state_digest":"opaque"}';
    const callback = createScopedHostCallback(scope, async (body, { signal }) => {
      const response = await callbackService.fetch(
        new Request("http://callback.invalid/fail", {
          method: "POST",
          body,
          signal,
        }),
      );
      if (!response.ok) throw Error("callback service rejected operation");
      return response.text();
    });
    await assert.rejects(callback(request), (error) =>
      error.code === "host_callback_failed" &&
      !String(error).includes("callback.invalid"),
    );
    scope.abort();
    assert.equal(await scope.settled(), true);
  });

  test("workerd-service-host-callback-timeout-cancels-the-owner-operation", async () => {
    const scope = createEventScope({}, { cleanupTimeoutMs: 100 });
    let observedAbort = false;
    const callback = createScopedHostCallback(
      scope,
      async (body, { signal }) => {
        signal.addEventListener("abort", () => {
          observedAbort = true;
        }, { once: true });
        const response = await callbackService.fetch(
          new Request("http://callback.invalid/delay", {
            method: "POST",
            body,
            signal,
          }),
        );
        return response.text();
      },
      { timeoutMs: 5 },
    );
    await assert.rejects(callback('{"operation":"create"}'), /host_callback_timeout/);
    assert.equal(observedAbort, true);
    scope.abort();
    // A service binding may honour abort immediately or settle its delayed
    // response later. In either case the event scope, not Auth, owns it.
    assert.equal(await scope.settled(), true);
  });
}
