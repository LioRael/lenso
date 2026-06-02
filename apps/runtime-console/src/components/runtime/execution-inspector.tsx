import { ArrowRight, Copy, RotateCcw, X } from "lucide-react";

import type {
  RuntimeStory,
  ExecutionNode,
  TechnicalOperation,
} from "../../data/mock-runtime";
import {
  useExecutionTechnicalOperations,
  useStoryTechnicalOperations,
} from "../../hooks/use-runtime-queries";
import { cn } from "../../lib/cn";
import { formatRuntimeDuration, serviceColor } from "../../lib/runtime-style";
import {
  buildExecutionActivity,
  buildExecutionContext,
  buildExecutionFailures,
  buildExecutionPayload,
  executionInspectorTabs,
  getExecutionInspectorTabCounts,
  type ExecutionActivityItem,
  type ExecutionInspectorTab,
} from "./execution-inspector-model";
import {
  HorizontalScrollArea,
  HorizontalTabScroll,
} from "./horizontal-tab-scroll";
import { JsonViewer } from "./json-viewer";
import { useRuntimeConsole } from "./runtime-console-context";
import { RuntimeStatusBadge } from "./runtime-status-badge";
import {
  buildTechnicalOperationGroups,
  technicalOperationsStateLabel,
  type TechnicalOperationGroup,
  type TechnicalOperationView,
} from "./technical-operations-model";

export function ExecutionInspector({
  activeTab,
  onClearSelection,
  selectedNode,
  setActiveTab,
  story,
}: {
  story: RuntimeStory;
  selectedNode: ExecutionNode;
  activeTab: ExecutionInspectorTab;
  onClearSelection: () => void;
  setActiveTab: (tab: ExecutionInspectorTab) => void;
}) {
  const node = selectedNode;

  const parent = node.parentId
    ? story.nodes.find((item) => item.id === node.parentId)
    : null;
  const breadcrumb = buildBreadcrumb(story, node);
  const directChildCount = story.nodes.filter(
    (item) => item.parentId === node.id
  ).length;
  const tabCounts = getExecutionInspectorTabCounts(story, node);

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
            {formatRuntimeDuration(node.durationMs)}
          </span>
          <RuntimeStatusBadge
            className="shrink-0"
            status={node.status}
            variant="compact"
          />
          <span className="min-w-0 truncate">{directChildCount} children</span>
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
              {formatRuntimeDuration(parent.durationMs)}
            </span>
          ) : null}
        </div>
      </div>

      <div className="min-w-0 overflow-hidden border-b border-(--border-subtle) bg-[color-mix(in_srgb,var(--surface)_82%,var(--background))]">
        <HorizontalTabScroll>
          <div className="flex h-full w-max min-w-full items-stretch pr-10">
            {executionInspectorTabs.map((tab) => (
              <button
                className={cn(
                  "inline-flex h-full shrink-0 items-center gap-1.5 whitespace-nowrap border-b border-transparent px-2 font-mono text-[10px] font-semibold uppercase tracking-[0.06em] text-(--muted) transition hover:border-(--border) hover:text-(--secondary) disabled:text-(--muted-deep)",
                  activeTab === tab.id &&
                    "border-(--accent) bg-[color-mix(in_srgb,var(--accent)_5%,transparent)] text-(--foreground)"
                )}
                key={tab.id}
                onClick={() => setActiveTab(tab.id)}
                type="button"
              >
                <span>{tab.label}</span>
                {tabCounts[tab.id] > 0 ? (
                  <span
                    className={cn(
                      "grid h-4.5 min-w-4.5 place-items-center border px-1 font-mono text-[10px] leading-none shadow-[inset_0_1px_0_rgba(255,255,255,0.03)]",
                      activeTab === tab.id
                        ? "border-[color-mix(in_srgb,var(--accent)_30%,transparent)] bg-[color-mix(in_srgb,var(--accent)_12%,transparent)] text-(--accent)"
                        : "border-(--border-subtle) bg-(--background) text-(--muted)"
                    )}
                  >
                    {tabCounts[tab.id]}
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
  activeTab: ExecutionInspectorTab;
}) {
  const { openRetry } = useRuntimeConsole();
  const executionOperationsQuery = useExecutionTechnicalOperations(node.id);
  const storyOperationsQuery = useStoryTechnicalOperations(story.correlationId);

  if (activeTab === "overview") {
    return (
      <div className="font-mono text-xs">
        <SummaryCard node={node} story={story} />
        <KeyValueTable
          rows={[
            ["execution name", node.name],
            ["execution type", typeLabel(node)],
            ["status", node.status],
            ["duration", formatRuntimeDuration(node.durationMs)],
            ["start time", formatRuntimeDuration(node.startMs)],
            [
              "completion time",
              formatRuntimeDuration(node.startMs + node.durationMs),
            ],
            ["story id", story.id],
            ["correlation id", story.correlationId],
            ["retry count", Math.max(0, (node.attempts ?? 1) - 1)],
            ["attempt", `${node.attempts ?? 1}/${node.maxAttempts ?? 1}`],
            ["parent count", parentCount(story, node)],
            ["child count", childCount(story, node)],
            ["service", node.service],
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
              Retry execution
            </button>
          </div>
        ) : null}
      </div>
    );
  }

  if (activeTab === "activity") {
    return <ActivityList activity={buildExecutionActivity(story, node)} />;
  }

  if (activeTab === "payload") {
    return <PayloadPanel node={node} />;
  }

  if (activeTab === "failures") {
    return <FailurePanel failures={buildExecutionFailures(node)} node={node} />;
  }

  if (activeTab === "logs") {
    return <LogList node={node} />;
  }

  if (activeTab === "context") {
    const context = buildExecutionContext(story, node);
    return (
      <div className="grid min-w-full">
        <KeyValueTable rows={context.rows} />
        <RelatedExecutionList
          label="upstream references"
          nodes={context.upstream}
        />
        <RelatedExecutionList
          label="downstream references"
          nodes={context.downstream}
        />
        <JsonViewer
          defaultExpanded
          title="execution context"
          value={{
            attributes: node.attributes,
            context: node.context,
          }}
        />
      </div>
    );
  }

  return (
    <TechnicalPanel
      executionOperations={executionOperationsQuery.data ?? []}
      error={executionOperationsQuery.error ?? storyOperationsQuery.error}
      isError={executionOperationsQuery.isError || storyOperationsQuery.isError}
      isLoading={
        executionOperationsQuery.isLoading || storyOperationsQuery.isLoading
      }
      node={node}
      story={story}
      storyOperations={storyOperationsQuery.data ?? []}
    />
  );
}

function SummaryCard({
  node,
  story,
}: {
  story: RuntimeStory;
  node: ExecutionNode;
}) {
  return (
    <div className="border-b border-(--border-subtle) bg-[color-mix(in_srgb,var(--surface)_82%,var(--background))] p-3">
      <div className="flex min-w-0 items-start gap-2">
        <span
          className="mt-1 size-2 shrink-0 rounded-xs"
          style={{ backgroundColor: serviceColor(node.service) }}
        />
        <div className="min-w-0">
          <div className="truncate text-[13px] font-semibold text-(--foreground)">
            {node.name}
          </div>
          <div className="mt-1 flex min-w-0 flex-wrap items-center gap-1.5 text-[11px] text-(--muted)">
            <span>{typeLabel(node)}</span>
            <span>·</span>
            <span>{node.status}</span>
            <span>·</span>
            <span>{formatRuntimeDuration(node.durationMs)}</span>
          </div>
          <div className="mt-2 truncate text-[11px] text-(--muted-deep)">
            {story.correlationId}
          </div>
        </div>
      </div>
    </div>
  );
}

function TechnicalPanel({
  executionOperations,
  error,
  isError,
  isLoading,
  node,
  story,
  storyOperations,
}: {
  executionOperations: TechnicalOperation[];
  storyOperations: TechnicalOperation[];
  story: RuntimeStory;
  node: ExecutionNode;
  isLoading: boolean;
  isError: boolean;
  error: unknown;
}) {
  const groups = buildTechnicalOperationGroups({
    executionOperations,
    selectedNodeId: node.id,
    storyOperations,
    storyTimestamp: story.timestamp,
  });
  if (groups.length === 0 || isLoading || isError) {
    return (
      <div className="grid min-w-full">
        <EmptyRows
          label={technicalOperationsStateLabel({ error, isError, isLoading })}
        />
      </div>
    );
  }

  return (
    <div className="grid min-w-full">
      {groups.map((group) => (
        <TechnicalOperationGroupView group={group} key={group.id} />
      ))}
    </div>
  );
}

function TechnicalOperationGroupView({
  group,
}: {
  group: TechnicalOperationGroup;
}) {
  return (
    <section className="border-b border-(--border-subtle)">
      <div className="flex items-center gap-2 bg-(--sidebar) px-3 py-1.5 font-mono text-[11px] text-(--muted)">
        <span>{group.label}</span>
        <span className="rounded-xs border border-(--border-subtle) bg-(--background) px-1.5 py-0.5 text-[10px] text-(--muted)">
          {group.operations.length}
        </span>
      </div>
      {group.operations.map((operation) => (
        <TechnicalOperationRow operation={operation} key={operation.id} />
      ))}
    </section>
  );
}

function TechnicalOperationRow({
  operation,
}: {
  operation: TechnicalOperationView;
}) {
  return (
    <div className="border-t border-(--border-subtle) bg-(--background)">
      <div className="grid min-w-full grid-cols-[72px_minmax(180px,1fr)_72px_64px_58px] items-center gap-2 px-3 py-2 font-mono text-xs">
        <span className="w-fit rounded-xs border border-(--border-subtle) bg-(--elevated) px-1.5 py-0.5 text-[10px] font-semibold uppercase text-(--accent)">
          {operation.category}
        </span>
        <span
          className="min-w-0 truncate text-(--foreground)"
          title={operation.name}
        >
          {operation.name}
        </span>
        <span
          className={cn(
            "text-[11px]",
            operation.status === "error" ? "text-[#ef4444]" : "text-(--muted)"
          )}
        >
          {operation.status}
        </span>
        <span className="text-right text-[11px] text-(--muted)">
          {formatRuntimeDuration(operation.durationMs)}
        </span>
        <span className="text-right text-[11px] text-(--muted)">
          +{formatRuntimeDuration(operation.relativeStartMs)}
        </span>
      </div>
      <JsonViewer title="safe attributes" value={operation.safeAttributes} />
    </div>
  );
}

function KeyValueTable({ rows }: { rows: Array<[string, unknown]> }) {
  if (rows.length === 0) {
    return <EmptyRows label="No execution details recorded" />;
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

function ActivityList({ activity }: { activity: ExecutionActivityItem[] }) {
  if (activity.length === 0) {
    return <EmptyRows label="No activity recorded" />;
  }
  return (
    <div className="w-max min-w-full font-mono text-xs">
      {activity.map((item) => (
        <div
          className="grid w-max min-w-full grid-cols-[58px_minmax(220px,max-content)] gap-2 border-b border-(--border-subtle) px-3 py-2"
          key={item.id}
        >
          <span className="whitespace-nowrap text-(--muted)">
            +{formatRuntimeDuration(item.timestampMs)}
          </span>
          <div>
            <div className="whitespace-nowrap text-(--foreground)">
              {item.label}
            </div>
            <div className="whitespace-nowrap text-[11px] text-(--muted)">
              {item.detail ?? `${item.kind} · ${item.status}`}
            </div>
          </div>
        </div>
      ))}
    </div>
  );
}

function FailurePanel({
  failures,
  node,
}: {
  failures: ReturnType<typeof buildExecutionFailures>;
  node: ExecutionNode;
}) {
  if (failures.length === 0) {
    return <EmptyRows label="No failures recorded" />;
  }

  return (
    <div className="grid min-w-full">
      <KeyValueTable rows={failures.map((item) => [item.label, item.value])} />
      <KeyValueTable
        rows={[
          ["dead letter state", node.status === "dead" ? "dead" : "-"],
          ["retryability", node.retryable ? "retryable" : "not retryable"],
          ["failure timeline", node.logs.join("\n") || "-"],
        ]}
      />
    </div>
  );
}

function PayloadPanel({ node }: { node: ExecutionNode }) {
  const payload = buildExecutionPayload(node);
  const sections = [
    ["Input", payload.input],
    ["Output", payload.output],
    ["Metadata", payload.metadata],
  ] as const;
  const availableSections = sections.filter(([, value]) =>
    hasPanelValue(value)
  );

  if (availableSections.length === 0) {
    return <EmptyRows label="No payload captured for this execution." />;
  }

  return (
    <div className="grid min-w-full">
      {availableSections.map(([label, value], index) => (
        <JsonViewer
          defaultExpanded={index === 0}
          key={label}
          title={label}
          value={value}
        />
      ))}
    </div>
  );
}

function LogList({ node }: { node: ExecutionNode }) {
  if (node.logs.length === 0) {
    return <EmptyRows label="No logs captured for this execution yet." />;
  }
  return (
    <div className="w-max min-w-full font-mono text-xs">
      {node.logs.map((log, index) => (
        <div
          className="grid w-max min-w-full grid-cols-[44px_54px_minmax(220px,max-content)] gap-2 border-b border-(--border-subtle) px-3 py-1.5"
          key={`${log}-${index}`}
        >
          <span className="whitespace-nowrap text-(--muted)">
            +{formatRuntimeDuration(node.startMs + index * 12)}
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

function hasPanelValue(value: unknown) {
  if (value === undefined || value === null) {
    return false;
  }
  if (Array.isArray(value)) {
    return value.length > 0;
  }
  if (typeof value === "object") {
    return Object.keys(value).length > 0;
  }
  return true;
}

function RelatedExecutionList({
  label,
  nodes,
}: {
  label: string;
  nodes: ExecutionNode[];
}) {
  if (nodes.length === 0) {
    return <EmptyRows label={`No ${label}`} />;
  }

  return (
    <div className="w-max min-w-full border-b border-(--border-subtle) font-mono text-xs">
      <div className="bg-(--sidebar) px-3 py-1.5 text-(--muted)">{label}</div>
      {nodes.map((node) => (
        <div
          className="grid w-max min-w-full grid-cols-[124px_minmax(220px,max-content)] border-t border-(--border-subtle)"
          key={node.id}
        >
          <div className="px-3 py-1.5 text-(--muted)">{node.kind}</div>
          <div className="whitespace-pre-wrap px-3 py-1.5 text-(--secondary)">
            {node.name}
          </div>
        </div>
      ))}
    </div>
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

function parentCount(story: RuntimeStory, node: ExecutionNode) {
  return story.nodes.filter((item) => item.id === node.parentId).length;
}

function childCount(story: RuntimeStory, node: ExecutionNode) {
  return story.nodes.filter((item) => item.parentId === node.id).length;
}
