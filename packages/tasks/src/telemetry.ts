import {
  defaultTextMapGetter,
  defaultTextMapSetter,
  ROOT_CONTEXT,
  trace,
} from "@opentelemetry/api";
import { W3CTraceContextPropagator } from "@opentelemetry/core";

// Durable metadata has a fixed W3C format, independent of the host's HTTP propagator.
const propagation = new W3CTraceContextPropagator();

export interface TraceMetadata {
  readonly traceparent: string;
  readonly tracestate?: string;
}

/** Durable tracing is a bounded allowlist, never baggage, payload or identity. */
export function traceMetadata(value: unknown): TraceMetadata | undefined {
  if (!value || typeof value !== "object") return undefined;
  const { traceparent, tracestate } = value as Record<string, unknown>;
  if (
    typeof traceparent !== "string" ||
    !/^00-[0-9a-f]{32}-[0-9a-f]{16}-[0-9a-f]{2}$/.test(traceparent) ||
    /^00-0{32}-/.test(traceparent) ||
    /^00-[0-9a-f]{32}-0{16}-/.test(traceparent)
  )
    return undefined;
  return {
    traceparent,
    ...(typeof tracestate === "string" &&
    tracestate.length <= 512 &&
    /^[\x20-\x7e]*$/.test(tracestate)
      ? { tracestate }
      : {}),
  };
}

export function producerMetadata(): TraceMetadata | undefined {
  const carrier: Record<string, string> = {};
  const span = trace.getActiveSpan();
  if (!span) return undefined;
  propagation.inject(trace.setSpan(ROOT_CONTEXT, span), carrier, defaultTextMapSetter);
  return traceMetadata(carrier);
}

export function producerLinks(value: unknown) {
  const metadata = traceMetadata(value);
  const spanContext = metadata
    ? trace.getSpanContext(propagation.extract(ROOT_CONTEXT, metadata, defaultTextMapGetter))
    : undefined;
  return spanContext && trace.isSpanContextValid(spanContext) ? [{ context: spanContext }] : [];
}
