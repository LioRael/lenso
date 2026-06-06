import { ExternalLink, PlayCircle, RefreshCcw, X } from "lucide-react";
import { useMemo, useState } from "react";

import { JsonViewer } from "../components/runtime/json-viewer";
import { ResizeHandle } from "../components/runtime/resize-handle";
import { useRuntimeConsole } from "../components/runtime/runtime-console-context";
import { Button } from "../components/ui/button";
import { useListKeyboard } from "../hooks/use-list-keyboard";
import {
  type RuntimeAdminActionInvocation,
  useAdminActionInvocations,
} from "../hooks/use-runtime-queries";
import { cn } from "../lib/cn";
import { time } from "../lib/format";
import { runtimeConsoleDataSource } from "../lib/http-client";
import {
  type AdminActionAggregate,
  type AdminActionResultFilter,
  adminActionInspectorDetails,
  adminActionPrimarySummary,
  adminActionResultLabel,
  adminActionsPath,
  aggregateAdminActionInvocations,
  filterAdminActionInvocations,
  flattenAdminActionInvocationPages,
  nextAdminActionInvocationCursor,
  summarizeAdminActionInvocations,
} from "./admin-actions-model";
import {
  OperationsFilterBar,
  OperationsFilterChip,
  OperationsSearchInput,
} from "./operations-filter";
import {
  OperationsInspectorHeader,
  OperationsSectionTitle,
  OperationsStatusBanner,
} from "./operations-inspector";
import { useOperationsInspectorLayout } from "./operations-layout";
import { useOperationsSelection } from "./operations-selection";
import {
  OperationsLoadingRows,
  OperationsMessageRow,
} from "./operations-state";
import {
  OperationsAggregateRow,
  OperationsKeyValueRows,
  OperationsSelectableRow,
  OperationsTableHeader,
} from "./operations-table";
import { pushOperationsUrl } from "./operations-url-model";
import {
  readOperationsParamValue,
  useOperationsUrlPopState,
} from "./operations-url-state";

export function AdminActionsPage() {
  const { openStoryTarget } = useRuntimeConsole();
  const [query, setQuery] = useState(() => readOperationsParamValue("q"));
  const [moduleName, setModuleName] = useState(() =>
    readOperationsParamValue("module")
  );
  const [actionName, setActionName] = useState(() =>
    readOperationsParamValue("action")
  );
  const [capability, setCapability] = useState(() =>
    readOperationsParamValue("capability")
  );
  const [correlationId, setCorrelationId] = useState(() =>
    readOperationsParamValue("correlation_id")
  );
  const [result, setResult] = useState<AdminActionResultFilter>(() =>
    readOperationsParamValue("result", readAdminActionResult)
  );
  const [selectedId, setSelectedId] = useState(() =>
    readOperationsParamValue("selected")
  );
  const { inspectorWidth, resetLayout, resizeInspector } =
    useOperationsInspectorLayout({
      defaultWidth: 408,
      maxWidth: 620,
      minWidth: 340,
      storageKey: "runtime-console:admin-actions-layout",
    });
  const actionFilters = {
    actionName,
    capability,
    correlationId,
    limit: 100,
    moduleName,
    ...(result === "all" ? {} : { success: result === "success" }),
  };
  const actionsQuery = useAdminActionInvocations(actionFilters);
  const actions = useMemo(
    () => flattenAdminActionInvocationPages(actionsQuery.data?.pages),
    [actionsQuery.data]
  );
  const nextCursor = nextAdminActionInvocationCursor(actionsQuery.data?.pages);
  const visible = useMemo(
    () => filterAdminActionInvocations(actions, { query, result }),
    [actions, query, result]
  );
  const summary = useMemo(
    () => summarizeAdminActionInvocations(actions),
    [actions]
  );
  const moduleAggregates = useMemo(
    () => aggregateAdminActionInvocations(actions, "module", 5),
    [actions]
  );
  const actionAggregates = useMemo(
    () => aggregateAdminActionInvocations(actions, "action", 5),
    [actions]
  );
  const errorAggregates = useMemo(
    () => aggregateAdminActionInvocations(actions, "error", 5),
    [actions]
  );

  useOperationsUrlPopState([
    { name: "q", setValue: setQuery },
    { name: "module", setValue: setModuleName },
    { name: "action", setValue: setActionName },
    { name: "capability", setValue: setCapability },
    { name: "correlation_id", setValue: setCorrelationId },
    { name: "result", parse: readAdminActionResult, setValue: setResult },
    { name: "selected", setValue: setSelectedId },
  ]);

  const actionsUrl = (
    overrides: Partial<{
      actionName: string;
      capability: string;
      correlationId: string;
      moduleName: string;
      query: string;
      result: AdminActionResultFilter;
      selectedId: string;
    }> = {}
  ) =>
    adminActionsPath({
      actionName: overrides.actionName ?? actionName,
      capability: overrides.capability ?? capability,
      correlationId: overrides.correlationId ?? correlationId,
      moduleName: overrides.moduleName ?? moduleName,
      query: overrides.query ?? query,
      result: overrides.result ?? result,
      selectedId: overrides.selectedId ?? selectedId,
    });

  const pushActionsUrl = (overrides: Parameters<typeof actionsUrl>[0] = {}) =>
    pushOperationsUrl(actionsUrl(overrides));

  const { selected, selectedIndex, selectIndex, selectItem } =
    useOperationsSelection({
      currentPath: adminActionsPath({
        actionName,
        capability,
        correlationId,
        moduleName,
        query,
        result,
        selectedId,
      }),
      getId: (action) => action.id,
      items: visible,
      pathForSelectedId: (nextSelectedId) =>
        actionsUrl({ selectedId: nextSelectedId }),
      selectedId,
      setSelectedId,
    });
  useListKeyboard({
    items: visible,
    onOpen: selectItem,
    selectedIndex,
    setSelectedIndex: selectIndex,
  });

  return (
    <section
      className="grid h-full min-h-0 min-w-0 overflow-hidden bg-(--background) text-(--foreground)"
      style={{
        gridTemplateColumns: `minmax(0,1fr) 1px ${inspectorWidth}px`,
      }}
    >
      <main className="grid min-h-0 min-w-0 grid-rows-[auto_auto_auto_auto_minmax(0,1fr)] overflow-hidden border-r border-(--border-subtle)">
        <header className="border-b border-(--border-subtle) bg-(--surface) px-3 py-2">
          <div className="flex items-center gap-2">
            <PlayCircle className="text-(--accent)" size={14} />
            <h1 className="font-mono text-[13px] font-semibold">
              Admin Actions
            </h1>
            <span className="ml-auto font-mono text-[10px] text-(--muted)">
              {visible.length} actions / {runtimeConsoleDataSource()}
            </span>
          </div>
        </header>

        <div className="grid border-b border-(--border-subtle) bg-(--surface) md:grid-cols-4">
          {[
            ["total", summary.total],
            ["success", summary.success],
            ["failed", summary.failed],
            ["avg", formatDuration(summary.avgDurationMs)],
          ].map(([label, value]) => (
            <div
              className="grid grid-cols-[minmax(0,1fr)_auto] border-r border-(--border-subtle) px-3 py-2 font-mono text-[10px] last:border-r-0"
              key={label}
            >
              <span className="text-(--muted)">{label}</span>
              <span
                className={cn(
                  "text-[13px] font-semibold text-(--foreground)",
                  label === "failed" && summary.failed > 0 && "text-[#ef4444]"
                )}
              >
                {value}
              </span>
            </div>
          ))}
        </div>

        <div className="grid border-b border-(--border-subtle) bg-(--background) lg:grid-cols-3">
          <AggregatePanel
            onSelect={(key) => {
              pushActionsUrl({ moduleName: key, selectedId: "" });
              setModuleName(key);
            }}
            rows={moduleAggregates}
            title="module"
          />
          <AggregatePanel
            onSelect={(key) => {
              pushActionsUrl({ actionName: key, selectedId: "" });
              setActionName(key);
            }}
            rows={actionAggregates}
            title="action"
          />
          <AggregatePanel
            onSelect={(key) => {
              const next = key === "success" ? "" : key;
              pushActionsUrl({ query: next, selectedId: "" });
              setQuery(next);
            }}
            rows={errorAggregates}
            title="error"
          />
        </div>

        <OperationsFilterBar>
          {(["all", "success", "failed"] as const).map((item) => (
            <OperationsFilterChip
              active={result === item}
              key={item}
              onClick={() => {
                pushActionsUrl({ result: item, selectedId: "" });
                setResult(item);
              }}
            >
              {item}
            </OperationsFilterChip>
          ))}
          <FilterInput
            ariaLabel="Filter admin actions by module"
            onChange={setModuleName}
            placeholder="module"
            value={moduleName}
          />
          <FilterInput
            ariaLabel="Filter admin actions by action"
            onChange={setActionName}
            placeholder="action"
            value={actionName}
          />
          <FilterInput
            ariaLabel="Filter admin actions by capability"
            className="min-w-[220px]"
            onChange={setCapability}
            placeholder="capability"
            value={capability}
          />
          <FilterInput
            ariaLabel="Filter admin actions by correlation"
            className="min-w-[220px]"
            onChange={setCorrelationId}
            placeholder="correlation"
            value={correlationId}
          />
          <OperationsSearchInput
            ariaLabel="Search admin actions"
            className="w-[min(320px,32vw)]"
            onChange={setQuery}
            placeholder="label / request / error"
            value={query}
          />
          {hasFilters({
            actionName,
            capability,
            correlationId,
            moduleName,
            query,
          }) ? (
            <button
              aria-label="Clear admin action filters"
              className="grid size-6 place-items-center border border-(--border-subtle) bg-(--elevated) text-(--muted) hover:text-(--foreground)"
              onClick={() => {
                pushActionsUrl({
                  actionName: "",
                  capability: "",
                  correlationId: "",
                  moduleName: "",
                  query: "",
                  selectedId: "",
                });
                setActionName("");
                setCapability("");
                setCorrelationId("");
                setModuleName("");
                setQuery("");
              }}
              type="button"
            >
              <X size={12} />
            </button>
          ) : null}
        </OperationsFilterBar>

        <div className="min-h-0 overflow-auto">
          <OperationsTableHeader className="grid-cols-[92px_148px_170px_minmax(220px,1fr)_92px_164px_88px] gap-3">
            <span>result</span>
            <span>module</span>
            <span>action</span>
            <span>summary</span>
            <span>duration</span>
            <span>correlation</span>
            <span>occurred</span>
          </OperationsTableHeader>
          {actionsQuery.isLoading ? (
            <OperationsLoadingRows />
          ) : actionsQuery.isError ? (
            <OperationsMessageRow
              message={errorMessage(actionsQuery.error)}
              tone="error"
            />
          ) : visible.length === 0 ? (
            <OperationsMessageRow message="no admin actions matched" />
          ) : (
            visible.map((action) => (
              <OperationsSelectableRow
                className="min-h-14 grid-cols-[92px_148px_170px_minmax(220px,1fr)_92px_164px_88px] gap-3"
                isSelected={selected?.id === action.id}
                key={action.id}
                onClick={() => selectItem(action)}
              >
                <ResultPill action={action} />
                <span className="min-w-0">
                  <span className="block truncate text-(--foreground)">
                    {action.module_name}
                  </span>
                  <span className="block truncate text-[10px] text-(--muted)">
                    {action.capability ?? "-"}
                  </span>
                </span>
                <span className="min-w-0">
                  <span className="block truncate text-(--foreground)">
                    {action.label}
                  </span>
                  <span className="block truncate text-[10px] text-(--muted)">
                    {action.action_name}
                  </span>
                </span>
                <span className="min-w-0">
                  <span className="block truncate text-(--foreground)">
                    {adminActionPrimarySummary(action)}
                  </span>
                  <span className="block truncate text-[10px] text-(--muted)">
                    {action.request_id ?? action.id}
                  </span>
                </span>
                <span className="text-(--secondary)">
                  {formatDuration(action.duration_ms)}
                </span>
                <span className="truncate text-[10px] text-(--muted)">
                  {action.correlation_id}
                </span>
                <span className="text-right text-[10px] text-(--muted)">
                  {time(action.occurred_at)}
                </span>
              </OperationsSelectableRow>
            ))
          )}
          {visible.length > 0 ? (
            <div className="flex items-center gap-3 border-b border-(--border-subtle) bg-(--surface) px-3 py-2">
              <Button
                disabled={
                  !actionsQuery.hasNextPage || actionsQuery.isFetchingNextPage
                }
                onClick={() => actionsQuery.fetchNextPage()}
                variant="ghost"
              >
                {actionsQuery.isFetchingNextPage
                  ? "Loading"
                  : actionsQuery.hasNextPage
                    ? "Load More"
                    : "End"}
              </Button>
              <span className="truncate font-mono text-[10px] text-(--muted)">
                loaded {actions.length}
                {nextCursor ? ` / before ${nextCursor}` : " / complete"}
              </span>
            </div>
          ) : null}
        </div>
      </main>

      <ResizeHandle
        ariaLabel="Resize admin action inspector panel"
        onReset={resetLayout}
        onResize={resizeInspector}
      />

      <aside className="relative z-0 grid min-h-0 min-w-0 grid-rows-[auto_minmax(0,1fr)_auto] overflow-hidden bg-(--sidebar)">
        <InspectorHeader action={selected} />
        <div className="min-h-0 overflow-auto">
          {selected ? (
            <AdminActionInspector action={selected} />
          ) : (
            <OperationsMessageRow message="select an admin action" />
          )}
        </div>
        <div className="flex gap-2 border-t border-(--border-subtle) bg-(--surface) p-2">
          <Button
            disabled={!selected}
            onClick={() =>
              selected &&
              openStoryTarget({
                correlationId: selected.correlation_id,
                nodeIdCandidates: [selected.id, selected.request_id ?? ""],
                ...(selected.request_id
                  ? { requestId: selected.request_id }
                  : {}),
              })
            }
            variant="ghost"
          >
            <ExternalLink size={13} />
            Story
          </Button>
          <Button
            disabled={actionsQuery.isRefetching}
            onClick={() => actionsQuery.refetch()}
            variant="ghost"
          >
            <RefreshCcw size={13} />
            Refresh
          </Button>
        </div>
      </aside>
    </section>
  );
}

function AggregatePanel({
  onSelect,
  rows,
  title,
}: {
  onSelect: (key: string) => void;
  rows: AdminActionAggregate[];
  title: string;
}) {
  return (
    <section className="min-w-0 border-r border-(--border-subtle) last:border-r-0">
      <OperationsTableHeader className="grid-cols-[minmax(0,1fr)_48px_56px] gap-2">
        <span>{title}</span>
        <span>fail</span>
        <span>rate</span>
      </OperationsTableHeader>
      <div>
        {rows.length === 0 ? (
          <div className="px-3 py-2 font-mono text-[10px] text-(--muted)">
            empty
          </div>
        ) : (
          rows.map((row) => (
            <OperationsAggregateRow
              className="grid-cols-[minmax(0,1fr)_48px_56px] gap-2"
              key={row.key}
              onClick={() => onSelect(row.key)}
            >
              <span className="min-w-0 truncate text-(--foreground)">
                {row.key}
              </span>
              <span
                className={row.failed > 0 ? "text-[#ef4444]" : "text-(--muted)"}
              >
                {row.failed}/{row.total}
              </span>
              <span className="text-(--secondary)">
                {formatPercent(row.failureRate)}
              </span>
            </OperationsAggregateRow>
          ))
        )}
      </div>
    </section>
  );
}

function FilterInput({
  ariaLabel,
  className,
  onChange,
  placeholder,
  value,
}: {
  ariaLabel: string;
  className?: string;
  onChange: (value: string) => void;
  placeholder: string;
  value: string;
}) {
  return (
    <label
      className={cn(
        "flex h-6 min-w-[150px] items-center border border-(--border-subtle) bg-(--elevated) px-2 font-mono text-(--muted)",
        className
      )}
    >
      <input
        aria-label={ariaLabel}
        className="w-full bg-transparent text-[10px] text-(--foreground) outline-hidden placeholder:text-(--muted)"
        onChange={(event) => onChange(event.target.value)}
        placeholder={placeholder}
        value={value}
      />
    </label>
  );
}

function InspectorHeader({
  action,
}: {
  action: RuntimeAdminActionInvocation | null;
}) {
  return (
    <OperationsInspectorHeader
      eyebrow={action ? action.module_name : "Admin"}
      meta={
        action ? (
          <>
            <span className="truncate">{action.id}</span>
            <span>{formatDuration(action.duration_ms)}</span>
            <span>{adminActionResultLabel(action)}</span>
          </>
        ) : null
      }
      title={action ? action.label : "Admin Action"}
    />
  );
}

function AdminActionInspector({
  action,
}: {
  action: RuntimeAdminActionInvocation;
}) {
  const details = adminActionInspectorDetails(action);
  return (
    <div className="grid">
      <OperationsStatusBanner
        label={adminActionResultLabel(action)}
        summary={adminActionPrimarySummary(action)}
        tone={action.success ? "success" : "error"}
      />
      <OperationsSectionTitle>action</OperationsSectionTitle>
      <OperationsKeyValueRows
        rows={details.actionRows.map(([key, value]) => [
          key === "duration_ms" ? "duration" : key,
          key === "duration_ms" ? formatDuration(action.duration_ms) : value,
        ])}
      />
      <OperationsSectionTitle>lineage</OperationsSectionTitle>
      <OperationsKeyValueRows rows={details.lineageRows} />
      <JsonViewer defaultExpanded title="summaries" value={details.summaries} />
      {details.failure ? (
        <JsonViewer defaultExpanded title="failure" value={details.failure} />
      ) : null}
    </div>
  );
}

function ResultPill({ action }: { action: RuntimeAdminActionInvocation }) {
  const label = adminActionResultLabel(action);
  return (
    <span
      className={cn(
        "w-fit border px-1.5 py-0.5 text-[10px] uppercase",
        label === "success"
          ? "border-[color-mix(in_srgb,var(--success)_45%,transparent)] text-(--success)"
          : "border-[color-mix(in_srgb,var(--error)_55%,transparent)] text-(--error)"
      )}
    >
      {label}
    </span>
  );
}

function readAdminActionResult(value: string): AdminActionResultFilter {
  return value === "success" || value === "failed" ? value : "all";
}

function formatDuration(ms: number) {
  if (ms < 1000) {
    return `${ms}ms`;
  }
  return `${(ms / 1000).toFixed(1)}s`;
}

function formatPercent(value: number) {
  return `${Math.round(value * 100)}%`;
}

function errorMessage(error: unknown) {
  return error instanceof Error
    ? error.message
    : "failed to load admin actions";
}

function hasFilters(filters: Record<string, string>) {
  return Object.values(filters).some((value) => value.trim().length > 0);
}
