import { ExternalLink, Network, RefreshCcw, Search, X } from "lucide-react";
import { useEffect, useMemo, useState } from "react";

import { JsonViewer } from "../components/runtime/json-viewer";
import { ResizeHandle } from "../components/runtime/resize-handle";
import { useRuntimeConsole } from "../components/runtime/runtime-console-context";
import { Button } from "../components/ui/button";
import { useBrowserUrlPopState } from "../hooks/use-browser-url-state";
import { useListKeyboard } from "../hooks/use-list-keyboard";
import { usePersistedLayout } from "../hooks/use-persisted-layout";
import {
  type RuntimeRemoteProxyCall,
  useRemoteProxyCalls,
} from "../hooks/use-runtime-queries";
import { cn } from "../lib/cn";
import { time } from "../lib/format";
import { runtimeConsoleDataSource } from "../lib/http-client";
import {
  resizeOperationsInspectorWidth,
  type OperationsInspectorLayout,
} from "./operations-layout";
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
import {
  pushOperationsUrl,
  readOperationsParam,
  replaceOperationsUrl,
} from "./operations-url-model";
import {
  type RemoteProxyCallAggregate,
  type RemoteProxyCallResultFilter,
  aggregateRemoteProxyCalls,
  filterRemoteProxyCalls,
  flattenRemoteProxyCallPages,
  nextRemoteProxyCallCursor,
  remoteProxyCallsPath,
  remoteProxyCallModules,
  remoteProxyCallResultLabel,
  summarizeRemoteProxyCalls,
} from "./remote-proxy-calls-model";

const remoteProxyCallsLayoutDefaults = {
  inspectorWidth: 408,
} satisfies OperationsInspectorLayout;

export function RemoteProxyCallsPage() {
  const { openStory, openStoryTarget } = useRuntimeConsole();
  const [query, setQuery] = useState(() => readOperationsParam("q"));
  const [moduleName, setModuleName] = useState(() =>
    readOperationsParam("module")
  );
  const [correlationId, setCorrelationId] = useState(() =>
    readOperationsParam("correlation_id")
  );
  const [result, setResult] = useState<RemoteProxyCallResultFilter>(() =>
    readRemoteProxyCallResult(readOperationsParam("result"))
  );
  const [selectedId, setSelectedId] = useState(() =>
    readOperationsParam("selected")
  );
  const [layout, setLayout, resetLayout] = usePersistedLayout(
    "runtime-console:remote-proxy-calls-layout",
    remoteProxyCallsLayoutDefaults
  );
  const remoteProxyCallsLayout = {
    ...remoteProxyCallsLayoutDefaults,
    ...layout,
  };
  const remoteProxyCallFilters = {
    correlationId,
    limit: 100,
    moduleName,
    ...(result === "all" ? {} : { success: result === "success" }),
  };
  const remoteProxyCallsQuery = useRemoteProxyCalls(remoteProxyCallFilters);
  const calls = useMemo(
    () => flattenRemoteProxyCallPages(remoteProxyCallsQuery.data?.pages),
    [remoteProxyCallsQuery.data]
  );
  const nextCursor = nextRemoteProxyCallCursor(
    remoteProxyCallsQuery.data?.pages
  );
  const visible = useMemo(
    () => filterRemoteProxyCalls(calls, { query, result }),
    [calls, query, result]
  );
  const modules = useMemo(() => remoteProxyCallModules(calls), [calls]);
  const summary = useMemo(() => summarizeRemoteProxyCalls(calls), [calls]);
  const moduleAggregates = useMemo(
    () => aggregateRemoteProxyCalls(calls, "module", 5),
    [calls]
  );
  const errorAggregates = useMemo(
    () => aggregateRemoteProxyCalls(calls, "error", 5),
    [calls]
  );
  const statusAggregates = useMemo(
    () => aggregateRemoteProxyCalls(calls, "status", 5),
    [calls]
  );

  useBrowserUrlPopState((search) => {
    setQuery(search.get("q") ?? "");
    setModuleName(search.get("module") ?? "");
    setCorrelationId(search.get("correlation_id") ?? "");
    setResult(readRemoteProxyCallResult(search.get("result") ?? ""));
    setSelectedId(search.get("selected") ?? "");
  });

  const remoteCallsUrl = (
    overrides: Partial<{
      correlationId: string;
      moduleName: string;
      query: string;
      result: RemoteProxyCallResultFilter;
      selectedId: string;
    }> = {}
  ) =>
    remoteProxyCallsPath({
      correlationId: overrides.correlationId ?? correlationId,
      moduleName: overrides.moduleName ?? moduleName,
      query: overrides.query ?? query,
      result: overrides.result ?? result,
      selectedId: overrides.selectedId ?? selectedId,
    });

  const pushRemoteCallsUrl = (
    overrides: Parameters<typeof remoteCallsUrl>[0] = {}
  ) => pushOperationsUrl(remoteCallsUrl(overrides));

  useEffect(() => {
    if (visible.length === 0) {
      if (selectedId) {
        setSelectedId("");
      }
      return;
    }
    if (!visible.some((call) => call.id === selectedId)) {
      setSelectedId(visible[0]?.id ?? "");
    }
  }, [selectedId, visible]);

  useEffect(() => {
    replaceOperationsUrl(
      remoteProxyCallsPath({
        correlationId,
        moduleName,
        query,
        result,
        selectedId,
      })
    );
  }, [correlationId, moduleName, query, result, selectedId]);

  const selected = visible.find((call) => call.id === selectedId) ?? null;
  const selectedIndex = selected ? indexOf(visible, selected.id) : 0;
  const selectIndex = (index: number) => {
    const call = visible[index];
    if (call) {
      pushRemoteCallsUrl({ selectedId: call.id });
      setSelectedId(call.id);
    }
  };
  const resizeInspector = (deltaX: number) => {
    setLayout((current) => ({
      ...current,
      inspectorWidth: resizeOperationsInspectorWidth({
        currentWidth: current.inspectorWidth,
        defaultWidth: remoteProxyCallsLayoutDefaults.inspectorWidth,
        deltaX,
        maxWidth: 620,
        minWidth: 340,
      }),
    }));
  };

  useListKeyboard({
    items: visible,
    selectedIndex,
    setSelectedIndex: selectIndex,
    onOpen: (call) => {
      pushRemoteCallsUrl({ selectedId: call.id });
      setSelectedId(call.id);
    },
  });

  return (
    <section
      className="grid h-full min-h-0 min-w-0 overflow-hidden bg-(--background) text-(--foreground)"
      style={{
        gridTemplateColumns: `minmax(0,1fr) 1px ${remoteProxyCallsLayout.inspectorWidth}px`,
      }}
    >
      <main
        className="grid min-h-0 min-w-0 overflow-hidden border-r border-(--border-subtle)"
        style={{
          gridTemplateRows: correlationId
            ? "auto auto auto auto auto minmax(0,1fr)"
            : "auto auto auto auto minmax(0,1fr)",
        }}
      >
        <header className="border-b border-(--border-subtle) bg-(--surface) px-3 py-2">
          <div className="flex items-center gap-2">
            <Network className="text-(--accent)" size={14} />
            <h1 className="font-mono text-[13px] font-semibold">
              Remote Calls
            </h1>
            <span className="ml-auto font-mono text-[10px] text-(--muted)">
              {visible.length} calls / {runtimeConsoleDataSource()}
            </span>
          </div>
        </header>

        <div className="grid border-b border-(--border-subtle) bg-(--surface) md:grid-cols-5">
          {[
            ["total", summary.total],
            ["success", summary.success],
            ["failed", summary.failed],
            ["avg", formatDuration(summary.avgDurationMs)],
            ["p95", formatDuration(summary.p95DurationMs)],
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
              pushRemoteCallsUrl({ moduleName: key, selectedId: "" });
              setModuleName(key);
            }}
            rows={moduleAggregates}
            title="module"
          />
          <AggregatePanel
            onSelect={(key) => {
              const next = key === "success" ? "" : key;
              pushRemoteCallsUrl({ query: next, selectedId: "" });
              setQuery(next);
            }}
            rows={errorAggregates}
            title="error"
          />
          <AggregatePanel
            onSelect={(key) => {
              pushRemoteCallsUrl({ query: key, selectedId: "" });
              setQuery(key);
            }}
            rows={statusAggregates}
            title="status"
          />
        </div>

        {correlationId ? (
          <div className="flex h-8 items-center gap-2 border-b border-(--border-subtle) bg-[color-mix(in_srgb,var(--accent)_6%,var(--background))] px-3 font-mono text-[10px]">
            <span className="text-(--muted)">correlation</span>
            <span className="min-w-0 truncate text-(--foreground)">
              {correlationId}
            </span>
            <button
              className="ml-auto flex h-5 items-center gap-1 border border-(--border-subtle) bg-(--elevated) px-1.5 text-(--secondary) hover:text-(--foreground)"
              onClick={() => openStory(correlationId)}
              type="button"
            >
              <ExternalLink size={11} />
              Story
            </button>
            <button
              aria-label="Clear correlation filter"
              className="grid size-5 place-items-center border border-(--border-subtle) bg-(--elevated) text-(--muted) hover:text-(--foreground)"
              onClick={() => {
                pushRemoteCallsUrl({ correlationId: "", selectedId: "" });
                setCorrelationId("");
              }}
              type="button"
            >
              <X size={12} />
            </button>
          </div>
        ) : null}

        <div className="flex h-9 items-center gap-2 border-b border-(--border-subtle) bg-(--background) px-3">
          {(["all", "success", "failed"] as const).map((item) => (
            <button
              className={cn(
                "h-6 border px-2 font-mono text-[10px]",
                result === item
                  ? "border-[color-mix(in_srgb,var(--accent)_40%,transparent)] bg-(--accent-soft) text-(--accent)"
                  : "border-(--border-subtle) text-(--muted) hover:text-(--foreground)"
              )}
              key={item}
              onClick={() => {
                pushRemoteCallsUrl({ result: item, selectedId: "" });
                setResult(item);
              }}
              type="button"
            >
              {item}
            </button>
          ))}
          <label className="flex h-6 min-w-[160px] items-center border border-(--border-subtle) bg-(--elevated) px-2 font-mono text-(--muted)">
            <input
              aria-label="Filter remote calls by module"
              className="w-full bg-transparent text-[10px] text-(--foreground) outline-hidden placeholder:text-(--muted)"
              list="remote-proxy-call-modules"
              onChange={(event) => setModuleName(event.target.value)}
              placeholder="module"
              value={moduleName}
            />
            <datalist id="remote-proxy-call-modules">
              {modules.map((module) => (
                <option key={module} value={module}>
                  {module}
                </option>
              ))}
            </datalist>
          </label>
          <label className="flex h-6 min-w-[200px] items-center border border-(--border-subtle) bg-(--elevated) px-2 font-mono text-(--muted)">
            <input
              aria-label="Filter remote calls by correlation"
              className="w-full bg-transparent text-[10px] text-(--foreground) outline-hidden placeholder:text-(--muted)"
              onChange={(event) => setCorrelationId(event.target.value)}
              placeholder="correlation"
              value={correlationId}
            />
          </label>
          <label className="ml-auto flex h-6 w-[min(360px,38vw)] items-center gap-2 border border-(--border-subtle) bg-(--elevated) px-2 font-mono text-(--muted)">
            <Search size={12} />
            <input
              aria-label="Search remote calls"
              className="w-full bg-transparent text-[10px] text-(--foreground) outline-hidden placeholder:text-(--muted)"
              onChange={(event) => setQuery(event.target.value)}
              placeholder="route / request / correlation"
              value={query}
            />
          </label>
        </div>

        <div className="min-h-0 overflow-auto">
          <OperationsTableHeader className="grid-cols-[92px_148px_minmax(220px,1.2fr)_minmax(220px,1.2fr)_88px_164px_88px] gap-3">
            <span>result</span>
            <span>module</span>
            <span>route</span>
            <span>remote</span>
            <span>duration</span>
            <span>correlation</span>
            <span>occurred</span>
          </OperationsTableHeader>
          {remoteProxyCallsQuery.isLoading ? (
            <OperationsLoadingRows />
          ) : remoteProxyCallsQuery.isError ? (
            <OperationsMessageRow
              message={errorMessage(remoteProxyCallsQuery.error)}
              tone="error"
            />
          ) : visible.length === 0 ? (
            <OperationsMessageRow message="no remote calls matched" />
          ) : (
            visible.map((call) => {
              const isSelected = selected?.id === call.id;
              return (
                <OperationsSelectableRow
                  className="min-h-14 grid-cols-[92px_148px_minmax(220px,1.2fr)_minmax(220px,1.2fr)_88px_164px_88px] gap-3"
                  isSelected={isSelected}
                  key={call.id}
                  onClick={() => {
                    pushRemoteCallsUrl({ selectedId: call.id });
                    setSelectedId(call.id);
                  }}
                >
                  <ResultPill call={call} />
                  <span className="min-w-0">
                    <span className="block truncate text-(--foreground)">
                      {call.module_name}
                    </span>
                    <span className="block truncate text-[10px] text-(--muted)">
                      {call.capability ?? "-"}
                    </span>
                  </span>
                  <span className="min-w-0">
                    <span className="block truncate text-(--foreground)">
                      {call.method} {call.declared_path}
                    </span>
                    <span className="block truncate text-[10px] text-(--muted)">
                      {call.request_id}
                    </span>
                  </span>
                  <span className="min-w-0">
                    <span className="block truncate text-(--foreground)">
                      {formatRemoteStatus(call.remote_status)}{" "}
                      {call.remote_path}
                    </span>
                    <span className="block truncate text-[10px] text-(--muted)">
                      {call.error_code ?? "-"}
                    </span>
                  </span>
                  <span className="text-(--secondary)">
                    {formatDuration(call.duration_ms)}
                  </span>
                  <span className="truncate text-[10px] text-(--muted)">
                    {call.correlation_id}
                  </span>
                  <span className="text-right text-[10px] text-(--muted)">
                    {time(call.occurred_at)}
                  </span>
                </OperationsSelectableRow>
              );
            })
          )}
          {visible.length > 0 ? (
            <div className="flex items-center gap-3 border-b border-(--border-subtle) bg-(--surface) px-3 py-2">
              <Button
                disabled={
                  !remoteProxyCallsQuery.hasNextPage ||
                  remoteProxyCallsQuery.isFetchingNextPage
                }
                onClick={() => remoteProxyCallsQuery.fetchNextPage()}
                variant="ghost"
              >
                {remoteProxyCallsQuery.isFetchingNextPage
                  ? "Loading"
                  : remoteProxyCallsQuery.hasNextPage
                    ? "Load More"
                    : "End"}
              </Button>
              <span className="truncate font-mono text-[10px] text-(--muted)">
                loaded {calls.length}
                {nextCursor ? ` / before ${nextCursor}` : " / complete"}
              </span>
            </div>
          ) : null}
        </div>
      </main>

      <ResizeHandle
        ariaLabel="Resize remote call inspector panel"
        onReset={resetLayout}
        onResize={resizeInspector}
      />

      <aside className="relative z-0 grid min-h-0 min-w-0 grid-rows-[auto_minmax(0,1fr)_auto] overflow-hidden bg-(--sidebar)">
        <InspectorHeader call={selected} />
        <div className="min-h-0 overflow-auto">
          {selected ? (
            <RemoteCallInspector call={selected} />
          ) : (
            <OperationsMessageRow message="select a remote call" />
          )}
        </div>
        <div className="flex gap-2 border-t border-(--border-subtle) bg-(--surface) p-2">
          <Button
            disabled={!selected}
            onClick={() =>
              selected &&
              openStoryTarget({
                correlationId: selected.correlation_id,
                nodeIdCandidates: [
                  `remoteproxy_${selected.id}`,
                  selected.id,
                  selected.request_id,
                ],
                remoteProxyCallId: selected.id,
                requestId: selected.request_id,
              })
            }
            variant="ghost"
          >
            <ExternalLink size={13} />
            Story
          </Button>
          <Button
            disabled={remoteProxyCallsQuery.isRefetching}
            onClick={() => remoteProxyCallsQuery.refetch()}
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
  rows: RemoteProxyCallAggregate[];
  title: string;
}) {
  return (
    <section className="min-w-0 border-r border-(--border-subtle) last:border-r-0">
      <OperationsTableHeader className="grid-cols-[minmax(0,1fr)_48px_56px_64px] gap-2">
        <span>{title}</span>
        <span>fail</span>
        <span>rate</span>
        <span>p95</span>
      </OperationsTableHeader>
      <div>
        {rows.length === 0 ? (
          <div className="px-3 py-2 font-mono text-[10px] text-(--muted)">
            empty
          </div>
        ) : (
          rows.map((row) => (
            <OperationsAggregateRow
              className="grid-cols-[minmax(0,1fr)_48px_56px_64px] gap-2"
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
              <span className="text-(--muted)">
                {formatDuration(row.p95DurationMs)}
              </span>
            </OperationsAggregateRow>
          ))
        )}
      </div>
    </section>
  );
}

function InspectorHeader({ call }: { call: RuntimeRemoteProxyCall | null }) {
  return (
    <header className="border-b border-(--border-subtle) bg-(--surface) px-3 py-2 font-mono">
      <div className="mb-1 text-[9px] font-semibold uppercase tracking-[0.12em] text-(--accent)">
        {call ? call.module_name : "Remote"}
      </div>
      <div className="truncate text-[13px] font-semibold text-(--foreground)">
        {call ? `${call.method} ${call.declared_path}` : "No call selected"}
      </div>
      {call ? (
        <div className="mt-1 flex items-center gap-2 text-[10px] text-(--muted)">
          <span className="truncate">{call.id}</span>
          <span>{formatDuration(call.duration_ms)}</span>
          <span>{remoteProxyCallResultLabel(call)}</span>
        </div>
      ) : null}
    </header>
  );
}

function RemoteCallInspector({ call }: { call: RuntimeRemoteProxyCall }) {
  return (
    <div className="grid">
      <OperationsKeyValueRows
        rows={[
          ["result", remoteProxyCallResultLabel(call)],
          ["module", call.module_name],
          ["capability", call.capability ?? "-"],
          ["method", call.method],
          ["declared", call.declared_path],
          ["remote", call.remote_path],
          ["remote_status", formatRemoteStatus(call.remote_status)],
          ["duration", formatDuration(call.duration_ms)],
          ["request", call.request_id],
          ["correlation", call.correlation_id],
          ["trace", call.trace_id ?? "-"],
          ["span", call.span_id ?? "-"],
          ["retryable", String(call.retryable)],
          ["occurred", call.occurred_at],
          ["error_code", call.error_code ?? "-"],
        ]}
      />
      <JsonViewer
        defaultExpanded
        title="path params"
        value={call.path_params}
      />
      <JsonViewer title="error details" value={call.error_details} />
    </div>
  );
}

function ResultPill({ call }: { call: RuntimeRemoteProxyCall }) {
  const label = remoteProxyCallResultLabel(call);
  return (
    <span
      className={cn(
        "inline-flex h-5 w-[76px] items-center justify-center border px-1.5 font-mono text-[10px] font-semibold",
        call.success &&
          "border-[color-mix(in_srgb,#22c55e_34%,transparent)] bg-[color-mix(in_srgb,#22c55e_10%,transparent)] text-[#22c55e]",
        !call.success &&
          call.retryable &&
          "border-[color-mix(in_srgb,#f59e0b_34%,transparent)] bg-[color-mix(in_srgb,#f59e0b_10%,transparent)] text-[#f59e0b]",
        !call.success &&
          !call.retryable &&
          "border-[color-mix(in_srgb,var(--error)_35%,transparent)] bg-[color-mix(in_srgb,var(--error)_10%,transparent)] text-[#ef4444]"
      )}
    >
      {label}
    </span>
  );
}

function formatDuration(ms: number) {
  if (ms < 1000) {
    return `${ms}ms`;
  }
  return `${(ms / 1000).toFixed(1)}s`;
}

function formatRemoteStatus(status: number | null | undefined) {
  return status === null || status === undefined ? "-" : String(status);
}

function formatPercent(value: number) {
  return `${Math.round(value * 100)}%`;
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : "Remote calls unavailable";
}

function indexOf(items: RuntimeRemoteProxyCall[], id: string) {
  return Math.max(
    0,
    items.findIndex((item) => item.id === id)
  );
}

function readRemoteProxyCallResult(value: string): RemoteProxyCallResultFilter {
  return value === "success" || value === "failed" ? value : "all";
}
