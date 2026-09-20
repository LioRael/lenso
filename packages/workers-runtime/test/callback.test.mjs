import assert from "node:assert/strict";
import test from "node:test";
import { createScopedHostCallback } from "../callback.mjs";
import { createEventScope } from "../scope.mjs";

const deferred = () => {
  let resolve, reject;
  const promise = new Promise((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
};
const turn = () => new Promise((resolve) => setImmediate(resolve));

test("host callback forwards opaque Auth operation JSON without a binding identity", async () => {
  const scope = createEventScope();
  const seen = [];
  const callback = createScopedHostCallback(scope, async (request, { signal }) => {
    seen.push({ request, aborted: signal.aborted });
    return '{"outcome":"created"}';
  });
  for (const operation of ["create", "consume", "revoke"]) {
    const request = JSON.stringify({ operation, opaque_auth_material: "ciphertext" });
    assert.equal(await callback(request), '{"outcome":"created"}');
  }
  assert.deepEqual(seen, [
    { request: '{"operation":"create","opaque_auth_material":"ciphertext"}', aborted: false },
    { request: '{"operation":"consume","opaque_auth_material":"ciphertext"}', aborted: false },
    { request: '{"operation":"revoke","opaque_auth_material":"ciphertext"}', aborted: false },
  ]);
  scope.abort();
  assert.equal(await scope.settled(), true);
});

test("host callback normalizes backend failure without exposing backend detail", async () => {
  const scope = createEventScope();
  const callback = createScopedHostCallback(scope, () =>
    Promise.reject(Error("postgres://private-user:private-password@private-host")),
  );
  await assert.rejects(callback('{"operation":"consume"}'), (error) =>
    error.code === "host_callback_failed" &&
    !String(error).includes("private-password"),
  );
  scope.abort();
  assert.equal(await scope.settled(), true);
});

test("timeout aborts the native callback and keeps its late completion in owner cleanup", async () => {
  const scope = createEventScope({}, { cleanupTimeoutMs: 10 });
  const native = deferred();
  let aborted = 0;
  const callback = createScopedHostCallback(
    scope,
    (_request, { signal }) => {
      signal.addEventListener("abort", () => aborted++, { once: true });
      return { promise: native.promise, abort() {} };
    },
    { timeoutMs: 5 },
  );
  await assert.rejects(callback('{"operation":"create"}'), /host_callback_timeout/);
  assert.equal(aborted, 1);
  scope.abort();
  assert.equal(await scope.settled(), false);
  native.resolve('{"outcome":"created"}');
  await turn();
  assert.equal(await scope.settled(), false, "uncertain cleanup receipt is sticky");
});

test("scope cancellation fences a late callback result before it can reach Auth", async () => {
  const scope = createEventScope();
  const native = deferred();
  let aborted = false;
  const callback = createScopedHostCallback(scope, (_request, { signal }) => {
    signal.addEventListener("abort", () => {
      aborted = true;
    }, { once: true });
    return native.promise;
  });
  const result = callback('{"operation":"revoke"}');
  scope.abort();
  await assert.rejects(result, /host_callback_cancelled/);
  assert.equal(aborted, true);
  native.resolve('{"outcome":"revoked"}');
  assert.equal(await scope.settled(), true);
});

test("invalid callback configuration fails before native admission", () => {
  const scope = createEventScope();
  assert.throws(() => createScopedHostCallback(scope, null), /invoke function/);
  assert.throws(
    () => createScopedHostCallback(scope, () => Promise.resolve(), { timeoutMs: 0 }),
    /timeout/,
  );
});
