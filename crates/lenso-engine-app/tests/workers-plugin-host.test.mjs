import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { test } from 'node:test';
import { pathToFileURL } from 'node:url';

const sdk = process.env.LENSO_JS_ROOT;
if (!sdk) throw new Error('Set LENSO_JS_ROOT to the qualified JS source checkout');
const { createEventScope } = await import(pathToFileURL(path.join(sdk, 'packages/lenso-workers-runtime/scope.mjs')));
const source = fs.readFileSync(new URL('../assets/workers-plugin-host.mjs', import.meta.url), 'utf8')
  .replace('import { prepareWorkersRequestPlugin } from "./runtime/plugin.mjs";', '');
const { invokePlugin, stopPlugin } = await import('data:text/javascript;base64,' + Buffer.from(source).toString('base64'));

// Before the bridge used scope.operation, event abort could not reach a JS
// handler's signal: physical work remained pending until its full deadline.
test('event abort reaches JS physical work and cleanup drains it', async () => {
  const scope = createEventScope({}, { cleanupTimeoutMs: 100 });
  let stopped = false;
  const plugin = {
    async invokeRequest(_, __, context) {
      await new Promise(resolve => context.signal.addEventListener('abort', resolve, { once: true }));
      stopped = true;
      return { kind: 'runtime', failure: { kind: 'cancelled' } };
    },
  };
  const request = invokePlugin(scope, plugin, 'example@1', 'wait', '{}', () => false, 1000, 'request');
  scope.abort();
  assert.equal(JSON.parse(await request).kind, 'runtime');
  assert.equal(await scope.settled(), true);
  assert.equal(stopped, true);
});

// A late handler may check cancellation after Wasm closures were invalidated.
// The JS fence must answer without calling back into the retired Wasm owner.
test('invalidation fences cancellation callbacks and late continuations', async () => {
  const scope = createEventScope({}, { cleanupTimeoutMs: 10 });
  let complete, callbacks = 0, observed = false, continued = false;
  const plugin = { async invokeRequest(_, __, context) {
    await new Promise(resolve => { complete = resolve; });
    observed = context.cancelled;
    return { kind: 'success', value: null };
  }};
  invokePlugin(scope, plugin, 'example@1', 'wait', '{}', () => { callbacks++; return false; }, 1000, 'request')
    .then(() => { continued = true; });
  scope.invalidate();
  complete();
  assert.equal(await scope.settled(), true);
  assert.equal(observed, true);
  assert.equal(callbacks, 0);
  assert.equal(continued, false);
});

test('stop uses physical cleanup authority after request admission closes', async () => {
  const scope = createEventScope({}, { cleanupTimeoutMs: 100 });
  scope.abort();
  let stopped = false;
  await stopPlugin(scope, { async stop(context) {
    assert.equal(context.cancelled, false);
    assert.equal(context.signal.aborted, false);
    await Promise.resolve();
    stopped = true;
  }}, () => false, 100);
  assert.equal(await scope.settled(), true);
  assert.equal(stopped, true);
});

test('nested calls preserve correlation while keeping invocation authority distinct', async () => {
  const scope = createEventScope();
  const observed = [];
  const plugin = { async invokeRequest(_, __, context) {
    observed.push([context.requestId, context.kernelContextKey]);
    return { kind: 'success', value: null };
  }};
  await Promise.all(['call-one', 'call-two'].map(key =>
    invokePlugin(scope, plugin, 'example@1', 'call', '{}', () => false, 100, 'same-request', key)));
  assert.deepEqual(observed, [['same-request', 'call-one'], ['same-request', 'call-two']]);
  assert.equal(await scope.settled(), true);
});
