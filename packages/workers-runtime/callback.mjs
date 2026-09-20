/**
 * Bind a Host-owned asynchronous callback to one event scope.
 *
 * The callback deliberately treats its request and response as opaque strings.
 * A Plugin can therefore own its private wire contract while a Worker Host owns
 * the native resource, cancellation, timeout and late-cleanup behaviour.  This
 * is an internal Host seam, not a database driver and not a Hyperdrive binding.
 */
const failure = (code) => {
  const error = new Error(code);
  error.code = code;
  return error;
};

const validTimeout = (value) =>
  Number.isSafeInteger(value) && value > 0 && value <= 0x7fffffff;

function operation(value) {
  if (value && typeof value.promise?.then === "function") {
    return {
      promise: Promise.resolve(value.promise),
      abort: typeof value.abort === "function" ? value.abort : undefined,
    };
  }
  if (value && typeof value.then === "function")
    return { promise: Promise.resolve(value), abort: undefined };
  return { promise: Promise.reject(failure("host_callback_invalid_result")) };
}

/**
 * @param {ReturnType<import("./scope.mjs").createEventScope>} scope
 * @param {(request: string, options: { signal: AbortSignal }) => Promise<string> | { promise: Promise<string>, abort?: () => unknown }} invoke
 * @param {{ timeoutMs?: number }} options
 */
export function createScopedHostCallback(
  scope,
  invoke,
  { timeoutMs = 1000 } = {},
) {
  if (!scope || typeof scope.operation !== "function" || typeof scope.trackNative !== "function")
    throw new TypeError("host callback requires an event scope");
  if (typeof invoke !== "function")
    throw new TypeError("host callback requires an invoke function");
  if (!validTimeout(timeoutMs))
    throw new RangeError("host callback timeout must be a positive 32-bit duration");

  return (request) => {
    if (typeof request !== "string")
      return Promise.reject(failure("host_callback_invalid_request"));
    const controller = new AbortController();
    let abortNative = () => {};
    let finish;
    let timer;
    let completed = false;
    const complete = (resolve, value) => {
      if (completed) return;
      completed = true;
      clearTimeout(timer);
      if (resolve) finish.resolve(value);
      else finish.reject(value);
    };
    const result = new Promise((resolve, reject) => {
      finish = { resolve, reject };
    });
    const abort = (code = "host_callback_cancelled") => {
      try {
        controller.abort();
      } catch {
        // A callback implementation cannot block cleanup by throwing from abort.
      }
      try {
        abortNative();
      } catch {
        // The scope will classify native abort failure through its own receipt.
      }
      complete(false, failure(code));
    };

    return scope.operation(() => {
      let native;
      try {
        native = operation(invoke(request, { signal: controller.signal }));
      } catch {
        native = { promise: Promise.reject(failure("host_callback_failed")) };
      }
      abortNative = native.abort ?? (() => {});
      // This promise is intentionally separate from the projection returned to
      // the Plugin.  A timed-out or cancelled callback still belongs to the
      // owner scope until the actual native operation finishes.
      scope.trackNative(native.promise);
      native.promise.then(
        (value) => complete(true, value),
        () => complete(false, failure("host_callback_failed")),
      );
      timer = setTimeout(() => abort("host_callback_timeout"), timeoutMs);
      return { promise: result, abort };
    }).promise;
  };
}
