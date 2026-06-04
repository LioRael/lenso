import { Network, RefreshCcw, Search, X } from "lucide-react";
import { useEffect, useMemo, useState } from "react";

import { JsonViewer } from "../components/runtime/json-viewer";
import { ResizeHandle } from "../components/runtime/resize-handle";
import { Button } from "../components/ui/button";
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
  type RemoteProxyCallAggregate,
  type RemoteProxyCallResultFilter,
  aggregateRemoteProxyCalls,
  filterRemoteProxyCalls,
  flattenRemoteProxyCallPages,
  nextRemoteProxyCallCursor,
  remoteProxyCallModules,
  remoteProxyCallResultLabel,
  summarizeRemoteProxyCalls,
} from "./remote-proxy-calls-model";

const remoteProxyCallsLayoutDefaults = {
  inspectorWidth: 408,
};

function clamp(value: number, min: number, max: number) {
  return Math.min(max, Math.max(min, value));
}

export function RemoteProxyCallsPage() {
  const [query, setQuery] = useState("");
  const [moduleName, setModuleName] = useState("");
  const [correlationId, setCorrelationId] = useState(() =>
    typeof window === "undefined"
      ? ""
      : (new URLSearchParams(window.location.search).get("correlation_id") ??
        "")
  );
  const [result, setResult] = useState<RemoteProxyCallResultFilter>("all");
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
  const [selectedIndex, setSelectedIndex] = useState(0);
  useEffect(
    () => setSelectedIndex(0),
    [correlationId, moduleName, query, result]
  );

  useEffect(() => {
    if (typeof window === "undefined") {
      return;
    }
    const url = new URL(window.location.href);
    if (correlationId) {
      url.searchParams.set("correlation_id", correlationId);
    } else {
      url.searchParams.delete("correlation_id");
    }
    window.history.replaceState(null, "", `${url.pathname}${url.search}`);
  }, [correlationId]);

  const selected = visible[selectedIndex] ?? null;
  const resizeInspector = (deltaX: number) => {
    setLayout((current) => ({
      ...current,
      inspectorWidth: clamp(
        (current.inspectorWidth ??
          remoteProxyCallsLayoutDefaults.inspectorWidth) - deltaX,
        340,
        620
      ),
    }));
  };

  useListKeyboard({
    items: visible,
    selectedIndex,
    setSelectedIndex,
    onOpen: (call) => setSelectedIndex(indexOf(visible, call.id)),
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
            onSelect={(key) => setModuleName(key)}
            rows={moduleAggregates}
            title="module"
          />
          <AggregatePanel
            onSelect={(key) => setQuery(key === "success" ? "" : key)}
            rows={errorAggregates}
            title="error"
          />
          <AggregatePanel
            onSelect={(key) => setQuery(key)}
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
              aria-label="Clear correlation filter"
              className="ml-auto grid size-5 place-items-center border border-(--border-subtle) bg-(--elevated) text-(--muted) hover:text-(--foreground)"
              onClick={() => setCorrelationId("")}
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
              onClick={() => setResult(item)}
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
          <div className="grid h-7 grid-cols-[92px_148px_minmax(220px,1.2fr)_minmax(220px,1.2fr)_88px_164px_88px] items-center gap-3 border-b border-(--border-subtle) bg-[color-mix(in_srgb,var(--elevated)_52%,transparent)] px-3 font-mono text-[9px] uppercase tracking-[0.08em] text-(--muted)">
            <span>result</span>
            <span>module</span>
            <span>route</span>
            <span>remote</span>
            <span>duration</span>
            <span>correlation</span>
            <span>occurred</span>
          </div>
          {remoteProxyCallsQuery.isLoading ? (
            <LoadingRows />
          ) : remoteProxyCallsQuery.isError ? (
            <MessageRow
              message={errorMessage(remoteProxyCallsQuery.error)}
              tone="error"
            />
          ) : visible.length === 0 ? (
            <MessageRow message="no remote calls matched" />
          ) : (
            visible.map((call) => {
              const isSelected = selected?.id === call.id;
              return (
                <button
                  className={cn(
                    "grid min-h-14 w-full grid-cols-[92px_148px_minmax(220px,1.2fr)_minmax(220px,1.2fr)_88px_164px_88px] items-center gap-3 border-b border-(--border-subtle) px-3 text-left font-mono text-[11px]",
                    isSelected
                      ? "bg-(--accent-soft) shadow-[inset_2px_0_0_var(--accent)]"
                      : "hover:bg-(--elevated)"
                  )}
                  key={call.id}
                  onClick={() => setSelectedIndex(indexOf(visible, call.id))}
                  type="button"
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
                </button>
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
            <MessageRow message="select a remote call" />
          )}
        </div>
        <div className="flex gap-2 border-t border-(--border-subtle) bg-(--surface) p-2">
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
      <div className="grid h-7 grid-cols-[minmax(0,1fr)_48px_56px_64px] items-center gap-2 border-b border-(--border-subtle) bg-[color-mix(in_srgb,var(--elevated)_52%,transparent)] px-3 font-mono text-[9px] uppercase tracking-[0.08em] text-(--muted)">
        <span>{title}</span>
        <span>fail</span>
        <span>rate</span>
        <span>p95</span>
      </div>
      <div>
        {rows.length === 0 ? (
          <div className="px-3 py-2 font-mono text-[10px] text-(--muted)">
            empty
          </div>
        ) : (
          rows.map((row) => (
            <button
              className="grid h-8 w-full grid-cols-[minmax(0,1fr)_48px_56px_64px] items-center gap-2 border-b border-(--border-subtle) px-3 text-left font-mono text-[10px] hover:bg-(--elevated)"
              key={row.key}
              onClick={() => onSelect(row.key)}
              type="button"
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
            </button>
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
    <div className="grid gap-3 p-3">
      <KeyValueRows
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

function KeyValueRows({ rows }: { rows: Array<[string, string]> }) {
  return (
    <div className="border-y border-(--border-subtle) font-mono text-[11px]">
      {rows.map(([key, value]) => (
        <div
          className="grid grid-cols-[108px_minmax(0,1fr)] border-b border-(--border-subtle) last:border-b-0"
          key={key}
        >
          <div className="bg-(--sidebar) px-3 py-1.5 text-(--muted)">{key}</div>
          <div className="min-w-0 break-words px-3 py-1.5 text-(--secondary)">
            {value}
          </div>
        </div>
      ))}
    </div>
  );
}

function LoadingRows() {
  return (
    <>
      <div className="h-14 animate-pulse border-b border-(--border-subtle) bg-(--elevated)" />
      <div className="h-14 animate-pulse border-b border-(--border-subtle) bg-(--elevated)" />
      <div className="h-14 animate-pulse border-b border-(--border-subtle) bg-(--elevated)" />
    </>
  );
}

function MessageRow({
  message,
  tone = "muted",
}: {
  message: string;
  tone?: "error" | "muted";
}) {
  return (
    <div
      className={cn(
        "border-b border-(--border-subtle) px-3 py-3 font-mono text-[11px]",
        tone === "error" ? "text-[#ef4444]" : "text-(--muted)"
      )}
    >
      {message}
    </div>
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
