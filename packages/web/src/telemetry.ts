import {
  context,
  propagation,
  ROOT_CONTEXT,
  SpanKind,
  SpanStatusCode,
  trace,
  metrics,
  type TextMapGetter,
} from "@opentelemetry/api";
import type { Logger } from "@lenso/core";
import type { createRequestTask } from "./lifetime";

const headersGetter: TextMapGetter<Headers> = {
  keys: (headers) => [...headers.keys()],
  get: (headers, key) => headers.get(key) ?? undefined,
};

function collect<T>(action: () => T): T | undefined {
  try {
    return action();
  } catch {
    return undefined;
  }
}

export function requestTelemetry(
  request: Request,
  create: (failed: () => void) => ReturnType<typeof createRequestTask>,
  options: {
    instanceId: string;
    pluginId: string;
    logger?: Logger;
    requestLifetime?: boolean;
  },
): ReturnType<typeof createRequestTask> {
  const method = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"].includes(
    request.method,
  )
    ? request.method
    : "_OTHER";
  const fields = {
    instanceId: options.instanceId,
    pluginId: options.pluginId,
    operation: "web.request",
  };
  const attributes = {
    "lenso.instance.id": options.instanceId,
    "lenso.plugin.id": options.pluginId,
    "http.request.method": method,
  };
  // This span owns body, detached work and cleanup, not a second HTTP server span.
  const parent =
    collect(() =>
      options.requestLifetime
        ? propagation.extract(ROOT_CONTEXT, request.headers, headersGetter)
        : context.active(),
    ) ?? ROOT_CONTEXT;
  const span = options.requestLifetime
    ? collect(() =>
        trace
          .getTracer("lenso.web")
          .startSpan("web.lifetime", { kind: SpanKind.INTERNAL, attributes }, parent),
      )
    : undefined;
  const scope = (span ? collect(() => trace.setSpan(parent, span)) : undefined) ?? parent;
  let logger: Logger | undefined;
  try {
    logger = options.logger?.child(fields);
  } catch {}
  const meter = collect(() => metrics.getMeter("lenso.web"));
  const count = collect(() => meter?.createCounter("lenso.web.requests"));
  const duration = collect(() => meter?.createHistogram("lenso.web.duration", { unit: "s" }));
  const errors = collect(() => meter?.createCounter("lenso.web.errors"));
  const start = performance.now();
  let status = 0;
  let failed = false;
  const onAbort = () => collect(() => span?.addEvent("aborted"));
  let task: ReturnType<typeof createRequestTask> | undefined;
  let entered = false;
  const startTask = () => {
    entered = true;
    return (task = create(() => {
      failed = true;
      collect(() => span?.addEvent("lifetime_failed"));
    }));
  };
  try {
    context.with(scope, startTask);
  } catch (error) {
    if (entered && !task) throw error;
  }
  task ??= startTask();
  task.signal.addEventListener("abort", onAbort, { once: true });
  if (task.signal.aborted) onAbort();
  void task.response
    .then((response) => {
      status = response.status;
      failed ||= status >= 500;
      collect(() => span?.setAttribute("http.response.status_code", status));
      collect(() => span?.addEvent("response_ready"));
    })
    .catch(() => {});
  const current = task;
  void Promise.all([task.response, task.completed])
    .then(() => {
      current.signal.removeEventListener("abort", onAbort);
      collect(() =>
        context.with(scope, () => {
          const labels = { "http.request.method": method, "http.response.status_code": status };
          try {
            count?.add(1, labels);
            duration?.record((performance.now() - start) / 1000, labels);
            if (failed) errors?.add(1, { "http.request.method": method });
            if (failed) span?.setStatus({ code: SpanStatusCode.ERROR });
            span?.addEvent("cleanup_complete");
            logger?.info({ status, failed }, "Web request finalized");
          } catch {
            // Diagnostics must not break request finalization.
          } finally {
            collect(() => span?.end());
          }
        }),
      );
    })
    .catch(() => {});
  return task;
}
