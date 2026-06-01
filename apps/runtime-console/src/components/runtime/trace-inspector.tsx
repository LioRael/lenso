import { ArrowRight, Copy, RotateCcw, X } from "lucide-react";

import type { RuntimeStory, ExecutionNode } from "../../data/mock-runtime";
import { cn } from "../../lib/cn";
import { formatTraceDuration, serviceColor } from "../../lib/trace-style";
import {
  HorizontalScrollArea,
  HorizontalTabScroll,
} from "./horizontal-tab-scroll";
import { JsonViewer } from "./json-viewer";
import { useRuntimeConsole } from "./runtime-console-context";
import { TraceStatusBadge } from "./trace-status-badge";

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
  selectedNode,
  setActiveTab,
  story,
}: {
  story: RuntimeStory;
  selectedNode: ExecutionNode;
  activeTab: InspectorTab;
  onClearSelection: () => void;
  setActiveTab: (tab: InspectorTab) => void;
}) {
  const node = selectedNode;

  const parent = node.parentId
    ? story.nodes.find((item) => item.id === node.parentId)
    : null;
  const breadcrumb = buildBreadcrumb(story, node);
  const childCount = story.nodes.filter(
    (item) => item.parentId === node.id
  ).length;
  const tabCounts = getTabCounts(node);

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
                {typeLabel(node)}
              </span>
              <span
                className="min-w-0 truncate rounded-xs border px-1.5 py-0.5 font-mono text-[10px] font-medium uppercase tracking-wide"
                style={{
                  backgroundColor: `${serviceColor(node.service)}14`,
                  borderColor: `${serviceColor(node.service)}28`,
                  color: serviceColor(node.service),
                }}
              >
                {node.service}
              </span>
            </div>
            <h2 className="truncate font-mono text-sm font-semibold leading-tight text-(--foreground)">
              {node.name}
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
          <button
            className="group flex min-w-10.5 flex-1 items-center gap-1 overflow-hidden text-left transition hover:text-(--secondary)"
            type="button"
          >
            <span className="truncate">{node.id.slice(0, 16)}</span>
            <Copy className="size-2.5 shrink-0 opacity-0 transition group-hover:opacity-100" />
          </button>
          <span className="shrink-0 text-(--accent)">
            {formatTraceDuration(node.durationMs)}
          </span>
          <TraceStatusBadge
            className="shrink-0"
            status={node.status}
            variant="compact"
          />
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
                      item.id === node.id
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
        <InspectorBody activeTab={activeTab} node={node} story={story} />
      </div>
    </aside>
  );
}

function InspectorBody({
  activeTab,
  node,
  story,
}: {
  story: RuntimeStory;
  node: ExecutionNode;
  activeTab: InspectorTab;
}) {
  const { openRetry } = useRuntimeConsole();

  if (activeTab === "info") {
    return (
      <div className="font-mono text-xs">
        <KeyValueTable
          rows={[
            ["status", node.status],
            ["duration", formatTraceDuration(node.durationMs)],
            ["service", node.service],
            ["kind", node.kind],
            ["start", formatTraceDuration(node.startMs)],
            ["end", formatTraceDuration(node.startMs + node.durationMs)],
            ["attempts", `${node.attempts ?? 1}/${node.maxAttempts ?? 1}`],
            ["correlation_id", story.correlationId],
            [
              "causation_id",
              String(node.context.causation_id ?? parentId(node) ?? "-"),
            ],
          ]}
        />
        {node.retryable ? (
          <div className="border-b border-(--border-subtle) px-3 py-2">
            <button
              className="inline-flex h-8 w-fit items-center gap-2 rounded-xs border border-[color-mix(in_srgb,var(--error)_35%,transparent)] bg-[color-mix(in_srgb,var(--error)_10%,transparent)] px-2 font-mono text-[11px] text-(--foreground) hover:bg-[color-mix(in_srgb,var(--error)_15%,transparent)]"
              onClick={() =>
                openRetry({
                  attempts: node.attempts ?? 1,
                  id: node.id,
                  kind: "timeline",
                  maxAttempts: node.maxAttempts ?? 3,
                  name: node.name,
                  status: node.status,
                })
              }
              type="button"
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
    return <KeyValueTable rows={objectRows(node.attributes)} />;
  }

  if (activeTab === "events") {
    return <EventList node={node} />;
  }

  if (activeTab === "errors") {
    return <ErrorPanel node={node} />;
  }

  if (activeTab === "logs") {
    return <LogList node={node} />;
  }

  return (
    <div className="grid min-w-full">
      <JsonViewer
        defaultExpanded
        title="payload / input"
        value={node.payload ?? {}}
      />
      <JsonViewer
        defaultExpanded
        title="actor / story context"
        value={{
          actor: node.context.actor ?? story.service,
          headers: node.context.headers ?? {},
          execution_context: {
            correlation_id: story.correlationId,
            parent_id: node.parentId ?? null,
            node_id: node.id,
            story_id: story.id,
          },
          ...node.context,
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

function EventList({ node }: { node: ExecutionNode }) {
  if (node.events.length === 0) {
    return <EmptyRows label="no events" />;
  }
  return (
    <div className="w-max min-w-full font-mono text-xs">
      {node.events.map((event) => (
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

function ErrorPanel({ node }: { node: ExecutionNode }) {
  const isError = node.status === "failed" || node.status === "dead";
  return (
    <KeyValueTable
      rows={[
        ["error_code", isError ? node.status : "-"],
        ["message", isError ? (node.logs.at(-1) ?? "runtime error") : "-"],
        ["stack / last_error", isError ? node.logs.join("\n") : "-"],
        ["retryability", node.retryable ? "retryable" : "not retryable"],
      ]}
    />
  );
}

function LogList({ node }: { node: ExecutionNode }) {
  if (node.logs.length === 0) {
    return <EmptyRows label="no logs" />;
  }
  return (
    <div className="w-max min-w-full font-mono text-xs">
      {node.logs.map((log, index) => (
        <div
          className="grid w-max min-w-full grid-cols-[44px_54px_minmax(220px,max-content)] gap-2 border-b border-(--border-subtle) px-3 py-1.5"
          key={`${log}-${index}`}
        >
          <span className="whitespace-nowrap text-(--muted)">
            +{formatTraceDuration(node.startMs + index * 12)}
          </span>
          <span
            className={cn(
              "uppercase",
              node.status === "failed" || node.status === "dead"
                ? "text-[#ef4444]"
                : "text-[#22c55e]"
            )}
          >
            {node.status === "failed" || node.status === "dead"
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

function buildBreadcrumb(story: RuntimeStory, node: ExecutionNode) {
  const path: ExecutionNode[] = [];
  let current: ExecutionNode | undefined = node;
  while (current) {
    path.unshift(current);
    const currentParentId: string | undefined = current.parentId;
    current = currentParentId
      ? story.nodes.find((item) => item.id === currentParentId)
      : undefined;
  }
  return path;
}

function typeLabel(node: ExecutionNode) {
  if (node.kind === "external") {
    return "provider";
  }
  if (node.kind === "function") {
    return "function";
  }
  if (node.kind === "http") {
    return "http";
  }
  if (node.kind === "event") {
    return "event";
  }
  return "node";
}

function parentId(node: ExecutionNode) {
  return node.parentId ?? null;
}

function getTabCounts(node: ExecutionNode): Record<InspectorTab, number> {
  const errorCount = node.status === "failed" || node.status === "dead" ? 1 : 0;

  return {
    attributes: Object.keys(node.attributes).length,
    context: Object.keys(node.context).length + (node.payload ? 1 : 0),
    errors: errorCount,
    events: node.events.length,
    info: 0,
    logs: node.logs.length,
  };
}
