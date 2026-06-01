import { ArrowRight, Copy, RotateCcw, X } from "lucide-react";

import type { TraceRun, TraceSpan } from "../../data/mock-runtime";
import { cn } from "../../lib/cn";
import {
  formatTraceDuration,
  serviceColor,
  statusColor,
} from "../../lib/trace-style";
import {
  HorizontalScrollArea,
  HorizontalTabScroll,
} from "./horizontal-tab-scroll";
import { JsonViewer } from "./json-viewer";
import { useRuntimeConsole } from "./runtime-console-context";

type InspectorTab =
  | "info"
  | "attributes"
  | "events"
  | "errors"
  | "logs"
  | "context";

const tabs: InspectorTab[] = [
  "info",
  "attributes",
  "events",
  "errors",
  "logs",
  "context",
];

export function TraceInspector({
  activeTab,
  onClearSelection,
  selectedSpan,
  setActiveTab,
  trace,
}: {
  trace: TraceRun;
  selectedSpan: TraceSpan;
  activeTab: InspectorTab;
  onClearSelection: () => void;
  setActiveTab: (tab: InspectorTab) => void;
}) {
  const span = selectedSpan;

  const parent = span.parentId
    ? trace.spans.find((item) => item.id === span.parentId)
    : null;
  const breadcrumb = buildBreadcrumb(trace, span);
  const childCount = trace.spans.filter(
    (item) => item.parentId === span.id
  ).length;
  const tabCounts = getTabCounts(span);

  return (
    <aside className="grid h-full min-h-0 w-full min-w-0 max-w-full grid-rows-[auto_auto_auto_minmax(0,1fr)] overflow-hidden bg-(--sidebar)">
      <div className="min-w-0 overflow-hidden border-b border-(--border-subtle) bg-(--surface)">
        <div className="flex min-w-0 items-start gap-2 px-3 py-2">
          <div className="min-w-0 flex-1">
            <div className="mb-1 flex min-w-0 items-center gap-1.5 overflow-hidden">
              <span
                className={cn(
                  "shrink-0 rounded-xs border px-1.5 py-0.5 font-mono text-[10px] font-semibold uppercase tracking-[0.08em]",
                  "border-(--border-subtle) bg-(--elevated) text-(--accent)"
                )}
              >
                {typeLabel(span)}
              </span>
              <span
                className="min-w-0 truncate rounded-xs border px-1.5 py-0.5 font-mono text-[10px] font-medium uppercase tracking-wide"
                style={{
                  backgroundColor: `${serviceColor(span.service)}14`,
                  borderColor: `${serviceColor(span.service)}28`,
                  color: serviceColor(span.service),
                }}
              >
                {span.service}
              </span>
            </div>
            <h2 className="truncate font-mono text-sm font-semibold leading-tight text-(--foreground)">
              {span.name}
            </h2>
          </div>
          <button
            aria-label="Clear inspector selection"
            className="grid size-6 shrink-0 place-items-center rounded-xs border border-(--border-subtle) bg-(--elevated) text-(--muted) transition hover:text-(--foreground)"
            onClick={onClearSelection}
            type="button"
          >
            <X size={13} />
          </button>
        </div>

        <div className="flex min-w-0 items-center gap-2 overflow-hidden border-t border-(--border-subtle) px-3 py-1.5 font-mono text-[11px] text-(--muted)">
          <button className="group flex min-w-10.5 flex-1 items-center gap-1 overflow-hidden text-left transition hover:text-(--secondary)">
            <span className="truncate">{span.id.slice(0, 16)}</span>
            <Copy className="size-2.5 shrink-0 opacity-0 transition group-hover:opacity-100" />
          </button>
          <span className="shrink-0 text-(--accent)">
            {formatTraceDuration(span.durationMs)}
          </span>
          <span
            className="shrink-0 rounded-xs border px-1.5 py-0.5 uppercase"
            style={{
              borderColor: `${statusColor(span.status)}40`,
              color: statusColor(span.status),
            }}
          >
            {span.status}
          </span>
          <span className="min-w-0 truncate">{childCount} children</span>
        </div>
      </div>

      <div className="min-w-0 overflow-hidden border-b border-(--border-subtle) bg-(--background) px-3 py-1.5">
        <div className="grid min-w-0 grid-cols-[minmax(0,1fr)_auto] items-center gap-2 font-mono text-[11px] text-(--muted)">
          <HorizontalScrollArea
            className="h-5"
            contentClassName="h-full"
            viewportClassName="h-full"
          >
            <div className="flex h-full w-max min-w-full items-center gap-1.5">
              <span className="shrink-0 text-(--muted-deep)">path</span>
              {breadcrumb.map((item, index) => (
                <span
                  className="flex shrink-0 items-center gap-1.5"
                  key={item.id}
                >
                  {index > 0 ? (
                    <ArrowRight className="size-3 shrink-0 text-(--muted-deep)" />
                  ) : null}
                  <span
                    className={cn(
                      item.id === span.id
                        ? "text-(--foreground)"
                        : "text-(--secondary)"
                    )}
                    title={item.name}
                  >
                    {item.name}
                  </span>
                </span>
              ))}
            </div>
          </HorizontalScrollArea>
          {parent ? (
            <span className="shrink-0 text-(--muted)">
              {formatTraceDuration(parent.durationMs)}
            </span>
          ) : null}
        </div>
      </div>

      <div className="min-w-0 overflow-hidden border-b border-(--border-subtle) bg-[color-mix(in_srgb,var(--surface)_82%,var(--background))]">
        <HorizontalTabScroll>
          <div className="flex h-full w-max min-w-full items-stretch pr-10">
            {tabs.map((tab) => (
              <button
                className={cn(
                  "inline-flex h-full shrink-0 items-center gap-1.5 whitespace-nowrap border-b border-transparent px-2 font-mono text-[10px] font-semibold uppercase tracking-[0.06em] text-(--muted) transition hover:border-(--border) hover:text-(--secondary) disabled:text-(--muted-deep)",
                  activeTab === tab &&
                    "border-(--accent) bg-[color-mix(in_srgb,var(--accent)_5%,transparent)] text-(--foreground)"
                )}
                key={tab}
                onClick={() => setActiveTab(tab)}
                type="button"
              >
                <span>{tab}</span>
                {tabCounts[tab] > 0 ? (
                  <span
                    className={cn(
                      "grid h-4.5 min-w-4.5 place-items-center border px-1 font-mono text-[10px] leading-none shadow-[inset_0_1px_0_rgba(255,255,255,0.03)]",
                      activeTab === tab
                        ? "border-[color-mix(in_srgb,var(--accent)_30%,transparent)] bg-[color-mix(in_srgb,var(--accent)_12%,transparent)] text-(--accent)"
                        : "border-(--border-subtle) bg-(--background) text-(--muted)"
                    )}
                  >
                    {tabCounts[tab]}
                  </span>
                ) : null}
              </button>
            ))}
          </div>
        </HorizontalTabScroll>
      </div>

      <div className="min-h-0 min-w-0 overflow-auto bg-(--background)">
        <InspectorBody activeTab={activeTab} span={span} trace={trace} />
      </div>
    </aside>
  );
}

function InspectorBody({
  activeTab,
  span,
  trace,
}: {
  trace: TraceRun;
  span: TraceSpan;
  activeTab: InspectorTab;
}) {
  const { openRetry } = useRuntimeConsole();

  if (activeTab === "info") {
    return (
      <div className="font-mono text-xs">
        <KeyValueTable
          rows={[
            ["status", span.status],
            ["duration", formatTraceDuration(span.durationMs)],
            ["service", span.service],
            ["kind", span.kind],
            ["start", formatTraceDuration(span.startMs)],
            ["end", formatTraceDuration(span.startMs + span.durationMs)],
            ["attempts", `${span.attempts ?? 1}/${span.maxAttempts ?? 1}`],
            ["correlation_id", trace.correlationId],
            [
              "causation_id",
              String(span.context.causation_id ?? parentId(span) ?? "-"),
            ],
          ]}
        />
        {span.retryable ? (
          <div className="border-b border-(--border-subtle) px-3 py-2">
            <button
              className="inline-flex h-8 w-fit items-center gap-2 rounded-xs border border-[color-mix(in_srgb,var(--error)_35%,transparent)] bg-[color-mix(in_srgb,var(--error)_10%,transparent)] px-2 font-mono text-[11px] text-(--foreground) hover:bg-[color-mix(in_srgb,var(--error)_15%,transparent)]"
              onClick={() =>
                openRetry({
                  attempts: span.attempts ?? 1,
                  id: span.id,
                  kind: "timeline",
                  maxAttempts: span.maxAttempts ?? 3,
                  name: span.name,
                  status: span.status,
                })
              }
            >
              <RotateCcw size={12} />
              Retry runtime item
            </button>
          </div>
        ) : null}
      </div>
    );
  }

  if (activeTab === "attributes") {
    return <KeyValueTable rows={objectRows(span.attributes)} />;
  }

  if (activeTab === "events") {
    return <EventList span={span} />;
  }

  if (activeTab === "errors") {
    return <ErrorPanel span={span} />;
  }

  if (activeTab === "logs") {
    return <LogList span={span} />;
  }

  return (
    <div className="grid min-w-full">
      <JsonViewer
        defaultExpanded
        title="payload / input"
        value={span.payload ?? {}}
      />
      <JsonViewer
        defaultExpanded
        title="actor / trace context"
        value={{
          actor: span.context.actor ?? trace.service,
          headers: span.context.headers ?? {},
          trace_context: {
            correlation_id: trace.correlationId,
            parent_id: span.parentId ?? null,
            span_id: span.id,
            trace_id: trace.id,
          },
          ...span.context,
        }}
      />
    </div>
  );
}

function KeyValueTable({ rows }: { rows: Array<[string, unknown]> }) {
  if (rows.length === 0) {
    return <EmptyRows label="no attributes" />;
  }

  return (
    <div className="w-max min-w-full border-b border-(--border-subtle) font-mono text-xs">
      {rows.map(([key, value]) => (
        <div
          className="grid w-max min-w-full grid-cols-[124px_minmax(220px,max-content)] border-b border-(--border-subtle) last:border-b-0"
          key={key}
        >
          <div className="bg-(--sidebar) px-3 py-1.5 text-(--muted)">{key}</div>
          <div className="whitespace-pre-wrap px-3 py-1.5 text-(--secondary)">
            {formatCell(value)}
          </div>
        </div>
      ))}
    </div>
  );
}

function EventList({ span }: { span: TraceSpan }) {
  if (span.events.length === 0) {
    return <EmptyRows label="no events" />;
  }
  return (
    <div className="w-max min-w-full font-mono text-xs">
      {span.events.map((event) => (
        <div
          className="grid w-max min-w-full grid-cols-[58px_minmax(220px,max-content)] gap-2 border-b border-(--border-subtle) px-3 py-2"
          key={`${event.name}-${event.timestampMs}`}
        >
          <span className="whitespace-nowrap text-(--muted)">
            +{formatTraceDuration(event.timestampMs)}
          </span>
          <div>
            <div className="whitespace-nowrap text-(--foreground)">
              {event.name}
            </div>
            <div className="whitespace-nowrap text-[11px] text-(--muted)">
              {event.attributes
                ? JSON.stringify(event.attributes)
                : "payload -"}
            </div>
          </div>
        </div>
      ))}
    </div>
  );
}

function ErrorPanel({ span }: { span: TraceSpan }) {
  const isError = span.status === "failed" || span.status === "dead";
  return (
    <KeyValueTable
      rows={[
        ["error_code", isError ? span.status : "-"],
        ["message", isError ? (span.logs.at(-1) ?? "runtime error") : "-"],
        ["stack / last_error", isError ? span.logs.join("\n") : "-"],
        ["retryability", span.retryable ? "retryable" : "not retryable"],
      ]}
    />
  );
}

function LogList({ span }: { span: TraceSpan }) {
  if (span.logs.length === 0) {
    return <EmptyRows label="no logs" />;
  }
  return (
    <div className="w-max min-w-full font-mono text-xs">
      {span.logs.map((log, index) => (
        <div
          className="grid w-max min-w-full grid-cols-[44px_54px_minmax(220px,max-content)] gap-2 border-b border-(--border-subtle) px-3 py-1.5"
          key={`${log}-${index}`}
        >
          <span className="whitespace-nowrap text-(--muted)">
            +{formatTraceDuration(span.startMs + index * 12)}
          </span>
          <span
            className={cn(
              "uppercase",
              span.status === "failed" || span.status === "dead"
                ? "text-[#ef4444]"
                : "text-[#22c55e]"
            )}
          >
            {span.status === "failed" || span.status === "dead"
              ? "error"
              : "info"}
          </span>
          <span className="whitespace-nowrap text-(--secondary)">{log}</span>
        </div>
      ))}
    </div>
  );
}

function EmptyRows({ label }: { label: string }) {
  return <div className="p-4 font-mono text-xs text-(--muted)">{label}</div>;
}

function objectRows(value: Record<string, unknown>) {
  return Object.entries(value).sort(([left], [right]) =>
    left.localeCompare(right)
  );
}

function formatCell(value: unknown) {
  if (typeof value === "string") {
    return value;
  }
  if (value === null || value === undefined) {
    return "-";
  }
  if (typeof value === "number" || typeof value === "boolean") {
    return String(value);
  }
  return JSON.stringify(value);
}

function buildBreadcrumb(trace: TraceRun, span: TraceSpan) {
  const path: TraceSpan[] = [];
  let current: TraceSpan | undefined = span;
  while (current) {
    path.unshift(current);
    const currentParentId: string | undefined = current.parentId;
    current = currentParentId
      ? trace.spans.find((item) => item.id === currentParentId)
      : undefined;
  }
  return path;
}

function typeLabel(span: TraceSpan) {
  if (span.kind === "external") {
    return "provider";
  }
  if (span.kind === "function") {
    return "function";
  }
  if (span.kind === "http") {
    return "http";
  }
  if (span.kind === "event") {
    return "event";
  }
  return "span";
}

function parentId(span: TraceSpan) {
  return span.parentId ?? null;
}

function getTabCounts(span: TraceSpan): Record<InspectorTab, number> {
  const errorCount = span.status === "failed" || span.status === "dead" ? 1 : 0;

  return {
    attributes: Object.keys(span.attributes).length,
    context: Object.keys(span.context).length + (span.payload ? 1 : 0),
    errors: errorCount,
    events: span.events.length,
    info: 0,
    logs: span.logs.length,
  };
}
