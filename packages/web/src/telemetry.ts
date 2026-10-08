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
  const parent = options.requestLifetime
    ? propagation.extract(ROOT_CONTEXT, request.headers, headersGetter)
    : context.active();
  const span = options.requestLifetime
    ? trace
        .getTracer("lenso.web")
        .startSpan("web.lifetime", { kind: SpanKind.INTERNAL, attributes }, parent)
    : undefined;
  const scope = span ? trace.setSpan(parent, span) : parent;
  let logger: Logger | undefined;
  try {
    logger = options.logger?.child(fields);
  } catch {}
  const meter = metrics.getMeter("lenso.web");
  const count = meter.createCounter("lenso.web.requests");
  const duration = meter.createHistogram("lenso.web.duration", { unit: "s" });
  const errors = meter.createCounter("lenso.web.errors");
  const start = performance.now();
  let status = 0;
  let failed = false;
  const onAbort = () => span?.addEvent("aborted");
  const task = context.with(scope, () =>
    create(() => {
      failed = true;
      span?.addEvent("lifetime_failed");
    }),
  );
  task.signal.addEventListener("abort", onAbort, { once: true });
  if (task.signal.aborted) onAbort();
  void task.response.then((response) => {
    status = response.status;
    failed ||= status >= 500;
    span?.setAttribute("http.response.status_code", status);
    span?.addEvent("response_ready");
  });
  void Promise.all([task.response, task.completed]).then(() =>
    context.with(scope, () => {
      task.signal.removeEventListener("abort", onAbort);
      const labels = { "http.request.method": method, "http.response.status_code": status };
      try {
        count.add(1, labels);
        duration.record((performance.now() - start) / 1000, labels);
        if (failed) errors.add(1, { "http.request.method": method });
        if (failed) span?.setStatus({ code: SpanStatusCode.ERROR });
        span?.addEvent("cleanup_complete");
        logger?.info({ status, failed }, "Web request finalized");
      } catch {
        // Diagnostics must not break request finalization.
      } finally {
        span?.end();
      }
    }),
  );
  return task;
}
