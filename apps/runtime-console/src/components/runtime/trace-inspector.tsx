import { ArrowRight, Copy, RotateCcw, X } from "lucide-react";

import type { TraceRun, TraceSpan } from "../../data/mock-runtime";
import { cn } from "../../lib/cn";
import {
  formatTraceDuration,
  serviceColor,
  statusColor,
} from "../../lib/trace-style";
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
  selectedSpan: TraceSpan | null;
  activeTab: InspectorTab;
  onClearSelection: () => void;
  setActiveTab: (tab: InspectorTab) => void;
}) {
  const span = selectedSpan;
  const parent = span?.parentId
    ? trace.spans.find((item) => item.id === span.parentId)
    : null;
  const breadcrumb = span ? buildBreadcrumb(trace, span) : [];
  const childCount = span
    ? trace.spans.filter((item) => item.parentId === span.id).length
    : 0;

  return (
    <aside className="grid h-full min-h-0 min-w-0 grid-rows-[auto_auto_auto_minmax(0,1fr)] overflow-hidden bg-[var(--sidebar)]">
      <div className="border-b border-[var(--border-subtle)] bg-[var(--surface)]">
        <div className="flex items-start gap-2 px-3 py-2">
          <div className="min-w-0 flex-1">
            <div className="mb-1 flex items-center gap-1.5">
              <span
                className={cn(
                  "rounded-[2px] border px-1.5 py-0.5 font-mono text-[10px] font-semibold uppercase tracking-[0.08em]",
                  span
                    ? "border-[var(--border-subtle)] bg-[var(--elevated)] text-[var(--accent)]"
                    : "border-[var(--border-subtle)] bg-[var(--background)] text-[var(--muted)]"
                )}
              >
                {span ? typeLabel(span) : "Inspector"}
              </span>
              {span ? (
                <span
                  className="rounded-[2px] border px-1.5 py-0.5 font-mono text-[10px] font-medium uppercase tracking-wide"
                  style={{
                    backgroundColor: `${serviceColor(span.service)}14`,
                    borderColor: `${serviceColor(span.service)}28`,
                    color: serviceColor(span.service),
                  }}
                >
                  {span.service}
                </span>
              ) : null}
            </div>
            <h2 className="truncate font-mono text-sm font-semibold leading-tight text-[var(--foreground)]">
              {span?.name ?? "No runtime item selected"}
            </h2>
          </div>
          <button
            aria-label="Clear inspector selection"
            className="grid size-6 flex-shrink-0 place-items-center rounded-[2px] border border-[var(--border-subtle)] bg-[var(--elevated)] text-[var(--muted)] transition hover:text-[var(--foreground)]"
            disabled={!span}
            onClick={onClearSelection}
            type="button"
          >
            <X size={13} />
          </button>
        </div>

        {span ? (
          <div className="grid grid-cols-[minmax(0,1fr)_auto_auto_auto] items-center gap-2 border-t border-[var(--border-subtle)] px-3 py-1.5 font-mono text-[11px] text-[var(--muted)]">
            <button className="group flex min-w-0 items-center gap-1 text-left transition hover:text-[var(--secondary)]">
              <span className="truncate">{span.id.slice(0, 16)}</span>
              <Copy className="size-2.5 opacity-0 transition group-hover:opacity-100" />
            </button>
            <span className="text-[var(--accent)]">
              {formatTraceDuration(span.durationMs)}
            </span>
            <span
              className="rounded-[2px] border px-1.5 py-0.5 uppercase"
              style={{
                borderColor: `${statusColor(span.status)}40`,
                color: statusColor(span.status),
              }}
            >
              {span.status}
            </span>
            <span>{childCount} children</span>
          </div>
        ) : null}
      </div>

      <div className="border-b border-[var(--border-subtle)] bg-[var(--background)] px-3 py-1.5">
        {span ? (
          <div className="flex min-w-0 items-center gap-1.5 overflow-hidden font-mono text-[11px] text-[var(--muted)]">
            <span className="flex-shrink-0 text-[var(--muted-deep)]">path</span>
            {breadcrumb.map((item, index) => (
              <span className="flex min-w-0 items-center gap-1.5" key={item.id}>
                {index > 0 ? (
                  <ArrowRight className="size-3 flex-shrink-0 text-[var(--muted-deep)]" />
                ) : null}
                <span
                  className={cn(
                    "truncate",
                    item.id === span.id ? "text-[var(--foreground)]" : "text-[var(--secondary)]"
                  )}
                  title={item.name}
                >
                  {item.name}
                </span>
              </span>
            ))}
            {parent ? (
              <span className="ml-auto flex-shrink-0 text-[var(--muted)]">
                {formatTraceDuration(parent.durationMs)}
              </span>
            ) : null}
          </div>
        ) : (
          <div className="font-mono text-[11px] text-[var(--muted-deep)]">
            select a span from the workbench to inspect runtime context
          </div>
        )}
      </div>

      <div className="flex h-8 border-b border-[var(--border-subtle)] bg-[var(--surface)] px-2 pt-1">
        {tabs.map((tab) => (
          <button
            className={cn(
              "border border-transparent border-b-0 px-2 font-mono text-[11px] font-semibold uppercase tracking-[0.04em] text-[var(--muted)] transition hover:bg-[var(--elevated)] hover:text-[var(--foreground)]",
              activeTab === tab &&
                "border-[var(--border-subtle)] bg-[var(--elevated)] text-[var(--accent)]"
            )}
            disabled={!span}
            key={tab}
            onClick={() => setActiveTab(tab)}
            type="button"
          >
            {tab}
          </button>
        ))}
      </div>

      <div className="min-h-0 overflow-auto bg-[var(--background)]">
        {span ? (
          <InspectorBody activeTab={activeTab} span={span} trace={trace} />
        ) : (
          <div className="p-3 font-mono">
            <div className="border-y border-[var(--border-subtle)]">
              {[
                ["status", "-"],
                ["duration", "-"],
                ["service", "-"],
                ["kind", "-"],
                ["trace", trace.id.slice(0, 16)],
              ].map(([label, value]) => (
                <div
                  className="grid grid-cols-[104px_minmax(0,1fr)] border-b border-[var(--border-subtle)] text-xs last:border-b-0"
                  key={label}
                >
                  <div className="bg-[var(--sidebar)] px-3 py-1.5 text-[var(--muted-deep)]">
                    {label}
                  </div>
                  <div className="px-3 py-1.5 text-[var(--muted)]">{value}</div>
                </div>
              ))}
            </div>
            <div className="mt-3 text-[11px] leading-5 text-[var(--muted-deep)]">
              select a bar, bucket, segment, or node to inspect runtime detail.
            </div>
          </div>
        )}
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
      <div className="grid gap-3 p-3 font-mono text-xs">
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
          <button
            className="inline-flex h-8 w-fit items-center gap-2 rounded-[2px] border border-[color-mix(in_srgb,var(--error)_35%,transparent)] bg-[color-mix(in_srgb,var(--error)_10%,transparent)] px-2 font-mono text-[11px] text-[var(--foreground)] hover:bg-[color-mix(in_srgb,var(--error)_15%,transparent)]"
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
    <div className="grid gap-3 p-3">
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
    <div className="border-y border-[var(--border-subtle)] font-mono text-xs">
      {rows.map(([key, value]) => (
        <div
          className="grid grid-cols-[124px_minmax(0,1fr)] border-b border-[var(--border-subtle)] last:border-b-0"
          key={key}
        >
          <div className="bg-[var(--sidebar)] px-3 py-1.5 text-[var(--muted)]">{key}</div>
          <div className="min-w-0 break-words px-3 py-1.5 text-[var(--secondary)]">
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
    <div className="font-mono text-xs">
      {span.events.map((event) => (
        <div
          className="grid grid-cols-[58px_minmax(0,1fr)] gap-2 border-b border-[var(--border-subtle)] px-3 py-2"
          key={`${event.name}-${event.timestampMs}`}
        >
          <span className="text-[var(--muted)]">
            +{formatTraceDuration(event.timestampMs)}
          </span>
          <div className="min-w-0">
            <div className="truncate text-[var(--foreground)]">{event.name}</div>
            <div className="truncate text-[11px] text-[var(--muted)]">
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
    <div className="grid gap-3 p-3 font-mono text-xs">
      <KeyValueTable
        rows={[
          ["error_code", isError ? span.status : "-"],
          ["message", isError ? (span.logs.at(-1) ?? "runtime error") : "-"],
          ["stack / last_error", isError ? span.logs.join("\n") : "-"],
          ["retryability", span.retryable ? "retryable" : "not retryable"],
        ]}
      />
    </div>
  );
}

function LogList({ span }: { span: TraceSpan }) {
  if (span.logs.length === 0) {
    return <EmptyRows label="no logs" />;
  }
  return (
    <div className="font-mono text-xs">
      {span.logs.map((log, index) => (
        <div
          className="grid grid-cols-[44px_54px_minmax(0,1fr)] gap-2 border-b border-[var(--border-subtle)] px-3 py-1.5"
          key={`${log}-${index}`}
        >
          <span className="text-[var(--muted)]">
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
          <span className="min-w-0 truncate text-[var(--secondary)]">{log}</span>
        </div>
      ))}
    </div>
  );
}

function EmptyRows({ label }: { label: string }) {
  return (
    <div className="p-4 font-mono text-xs text-[var(--muted)]">{label}</div>
  );
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
