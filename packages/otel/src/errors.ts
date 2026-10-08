import type { ReadableSpan, SpanExporter } from "@opentelemetry/sdk-trace-base";

function omitErrorAttributes(attributes: ReadableSpan["attributes"]): ReadableSpan["attributes"] {
  return Object.fromEntries(
    Object.entries(attributes).filter(
      ([key]) => !key.startsWith("exception.") && !key.startsWith("error."),
    ),
  );
}

function safeSpan(span: ReadableSpan): ReadableSpan {
  const spanContext = span.spanContext();
  return {
    name: span.name,
    kind: span.kind,
    spanContext: () => spanContext,
    parentSpanContext: span.parentSpanContext,
    startTime: span.startTime,
    endTime: span.endTime,
    duration: span.duration,
    status: { code: span.status.code },
    attributes: omitErrorAttributes(span.attributes),
    links: span.links.map((link) => ({
      ...link,
      attributes: link.attributes && omitErrorAttributes(link.attributes),
    })),
    events: span.events
      .filter((event) => event.name !== "exception")
      .map((event) => ({
        ...event,
        attributes: event.attributes && omitErrorAttributes(event.attributes),
      })),
    ended: span.ended,
    resource: span.resource,
    instrumentationScope: span.instrumentationScope,
    droppedAttributesCount: span.droppedAttributesCount,
    droppedEventsCount: span.droppedEventsCount,
    droppedLinksCount: span.droppedLinksCount,
  };
}

/**
 * Wrap every export pipeline at the SDK owner's boundary. This omits standard
 * exception/error details, not arbitrary secrets in names, resources or payloads.
 * Lifecycle methods delegate to the exporter; wrapping does not transfer ownership.
 */
export function createSafeSpanExporter(exporter: SpanExporter): SpanExporter {
  return new Proxy(exporter, {
    get(target, property) {
      if (property === "export") {
        return (...[spans, done]: Parameters<SpanExporter["export"]>) => {
          try {
            target.export(spans.map(safeSpan), done);
          } catch {
            done({ code: 1, error: new Error("Safe trace export failed") });
          }
        };
      }
      const value = Reflect.get(target, property, target);
      return typeof value === "function" ? value.bind(target) : value;
    },
  });
}
