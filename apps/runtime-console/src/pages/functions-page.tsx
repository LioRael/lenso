import {
  Braces,
  ExternalLink,
  RefreshCcw,
  RotateCcw,
  Search,
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";

import { JsonViewer } from "../components/runtime/json-viewer";
import { ResizeHandle } from "../components/runtime/resize-handle";
import { useRuntimeConsole } from "../components/runtime/runtime-console-context";
import { Button } from "../components/ui/button";
import { retryTargetFor, type FunctionRun } from "../data/mock-runtime";
import { useBrowserUrlPopState } from "../hooks/use-browser-url-state";
import { useListKeyboard } from "../hooks/use-list-keyboard";
import { usePersistedLayout } from "../hooks/use-persisted-layout";
import {
  useRuntimeFunctionDetail,
  useRuntimeFunctions,
} from "../hooks/use-runtime-queries";
import { cn } from "../lib/cn";
import { actorLabel, time } from "../lib/format";
import { runtimeConsoleDataSource } from "../lib/http-client";
import {
  aggregateFunctionRuns,
  distinctFunctionMetadata,
  filterFunctionRuns,
  formatFunctionDuration,
  functionStatusFilters,
  runDurationMs,
  summarizeFunctionRuns,
  type FunctionRunAggregate,
  type FunctionStatusFilter,
} from "./functions-model";
import {
  resizeOperationsInspectorWidth,
  type OperationsInspectorLayout,
} from "./operations-layout";
import {
  OperationsLoadingRows,
  OperationsMessageRow,
} from "./operations-state";
import {
  functionsPath,
  pushOperationsUrl,
  readOperationsParam,
  replaceOperationsUrl,
} from "./operations-url-model";

const functionsLayoutDefaults = {
  inspectorWidth: 408,
} satisfies OperationsInspectorLayout;

export function FunctionsPage() {
  const { openRetry, openStoryTarget } = useRuntimeConsole();
  const [query, setQuery] = useState(() => readOperationsParam("q"));
  const [status, setStatus] = useState<FunctionStatusFilter>(() =>
    readFunctionStatus(readOperationsParam("status"))
  );
  const [moduleName, setModuleName] = useState(() =>
    readOperationsParam("module")
  );
  const [queue, setQueue] = useState(() => readOperationsParam("queue"));
  const [selectedId, setSelectedId] = useState(() =>
    readOperationsParam("selected")
  );
  const [layout, setLayout, resetLayout] = usePersistedLayout(
    "runtime-console:functions-layout",
    functionsLayoutDefaults
  );
  const functionsLayout = { ...functionsLayoutDefaults, ...layout };
  const functionsQuery = useRuntimeFunctions();
  const runs = useMemo(() => functionsQuery.data ?? [], [functionsQuery.data]);
  const visible = useMemo(
    () => filterFunctionRuns(runs, { moduleName, query, queue, status }),
    [moduleName, query, queue, status, runs]
  );
  const modules = useMemo(
    () => distinctFunctionMetadata(runs, "module"),
    [runs]
  );
  const queues = useMemo(() => distinctFunctionMetadata(runs, "queue"), [runs]);
  const summary = useMemo(() => summarizeFunctionRuns(runs), [runs]);
  const moduleAggregates = useMemo(
    () => aggregateFunctionRuns(runs, "module", 5),
    [runs]
  );
  const queueAggregates = useMemo(
    () => aggregateFunctionRuns(runs, "queue", 5),
    [runs]
  );
  const statusAggregates = useMemo(
    () => aggregateFunctionRuns(runs, "status", 5),
    [runs]
  );

  useBrowserUrlPopState((search) => {
    setQuery(search.get("q") ?? "");
    setStatus(readFunctionStatus(search.get("status") ?? ""));
    setModuleName(search.get("module") ?? "");
    setQueue(search.get("queue") ?? "");
    setSelectedId(search.get("selected") ?? "");
  });

  const functionUrl = (
    overrides: Partial<{
      moduleName: string;
      query: string;
      queue: string;
      selectedId: string;
      status: FunctionStatusFilter;
    }> = {}
  ) =>
    functionsPath({
      moduleName: overrides.moduleName ?? moduleName,
      query: overrides.query ?? query,
      queue: overrides.queue ?? queue,
      selectedId: overrides.selectedId ?? selectedId,
      status: overrides.status ?? status,
    });

  const pushFunctionUrl = (overrides: Parameters<typeof functionUrl>[0] = {}) =>
    pushOperationsUrl(functionUrl(overrides));

  useEffect(() => {
    if (visible.length === 0) {
      if (selectedId) {
        setSelectedId("");
      }
      return;
    }
    if (!visible.some((run) => run.id === selectedId)) {
      setSelectedId(visible[0]?.id ?? "");
    }
  }, [selectedId, visible]);

  useEffect(() => {
    replaceOperationsUrl(
      functionsPath({ moduleName, query, queue, selectedId, status })
    );
  }, [moduleName, query, queue, selectedId, status]);

  const selected = visible.find((run) => run.id === selectedId) ?? null;
  const selectedIndex = selected ? indexOf(visible, selected.id) : 0;
  const selectIndex = (index: number) => {
    const run = visible[index];
    if (run) {
      pushFunctionUrl({ selectedId: run.id });
      setSelectedId(run.id);
    }
  };
  const resizeInspector = (deltaX: number) => {
    setLayout((current) => ({
      ...current,
      inspectorWidth: resizeOperationsInspectorWidth({
        currentWidth: current.inspectorWidth,
        defaultWidth: functionsLayoutDefaults.inspectorWidth,
        deltaX,
        maxWidth: 620,
        minWidth: 340,
      }),
    }));
  };
  const retryRun = (run: FunctionRun) => {
    const retryTarget = retryTargetFor({ kind: "function", item: run });
    if (retryTarget) {
      openRetry(retryTarget);
    }
  };

  useListKeyboard({
    items: visible,
    selectedIndex,
    setSelectedIndex: selectIndex,
    onOpen: (run) => {
      pushFunctionUrl({ selectedId: run.id });
      setSelectedId(run.id);
    },
    onRetry: retryRun,
  });

  return (
    <section
      className="grid h-full min-h-0 min-w-0 overflow-hidden bg-(--background) text-(--foreground)"
      style={{
        gridTemplateColumns: `minmax(0,1fr) 1px ${functionsLayout.inspectorWidth}px`,
      }}
    >
      <main className="grid min-h-0 min-w-0 grid-rows-[auto_auto_auto_auto_minmax(0,1fr)] overflow-hidden border-r border-(--border-subtle)">
        <header className="border-b border-(--border-subtle) bg-(--surface) px-3 py-2">
          <div className="flex items-center gap-2">
            <Braces className="text-(--accent)" size={14} />
            <h1 className="font-mono text-[13px] font-semibold">Functions</h1>
            <span className="ml-auto font-mono text-[10px] text-(--muted)">
              {visible.length} runs / {runtimeConsoleDataSource()}
            </span>
          </div>
        </header>

        <div className="grid border-b border-(--border-subtle) bg-(--surface) md:grid-cols-6">
          {[
            ["total", summary.total],
            ["pending", summary.pending],
            ["running", summary.running],
            ["completed", summary.completed],
            ["failed", summary.failed],
            ["dead", summary.dead],
          ].map(([label, value]) => (
            <div
              className="grid grid-cols-[minmax(0,1fr)_auto] border-r border-(--border-subtle) px-3 py-2 font-mono text-[10px] last:border-r-0"
              key={label}
            >
              <span className="text-(--muted)">{label}</span>
              <span
                className={cn(
                  "text-[13px] font-semibold text-(--foreground)",
                  (label === "failed" || label === "dead") &&
                    Number(value) > 0 &&
                    "text-[#ef4444]"
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
              const next = key === "undeclared" ? "" : key;
              pushFunctionUrl({ moduleName: next, selectedId: "" });
              setModuleName(next);
            }}
            rows={moduleAggregates}
            title="module"
          />
          <AggregatePanel
            onSelect={(key) => {
              const next = key === "undeclared" ? "" : key;
              pushFunctionUrl({ queue: next, selectedId: "" });
              setQueue(next);
            }}
            rows={queueAggregates}
            title="queue"
          />
          <AggregatePanel
            onSelect={(key) => {
              const next = readFunctionStatus(key);
              pushFunctionUrl({ selectedId: "", status: next });
              setStatus(next);
            }}
            rows={statusAggregates}
            title="status"
          />
        </div>

        <div className="flex h-9 items-center gap-2 border-b border-(--border-subtle) bg-(--background) px-3">
          {functionStatusFilters.map((item) => (
            <button
              className={cn(
                "h-6 border px-2 font-mono text-[10px]",
                status === item
                  ? "border-[color-mix(in_srgb,var(--accent)_40%,transparent)] bg-(--accent-soft) text-(--accent)"
                  : "border-(--border-subtle) text-(--muted) hover:text-(--foreground)"
              )}
              key={item}
              onClick={() => {
                pushFunctionUrl({ selectedId: "", status: item });
                setStatus(item);
              }}
              type="button"
            >
              {item}
            </button>
          ))}
          <label className="flex h-6 min-w-[150px] items-center border border-(--border-subtle) bg-(--elevated) px-2 font-mono text-(--muted)">
            <input
              aria-label="Filter functions by module"
              className="w-full bg-transparent text-[10px] text-(--foreground) outline-hidden placeholder:text-(--muted)"
              list="function-run-modules"
              onChange={(event) => setModuleName(event.target.value)}
              placeholder="module"
              value={moduleName}
            />
            <datalist id="function-run-modules">
              {modules.map((module) => (
                <option key={module} value={module}>
                  {module}
                </option>
              ))}
            </datalist>
          </label>
          <label className="flex h-6 min-w-[140px] items-center border border-(--border-subtle) bg-(--elevated) px-2 font-mono text-(--muted)">
            <input
              aria-label="Filter functions by queue"
              className="w-full bg-transparent text-[10px] text-(--foreground) outline-hidden placeholder:text-(--muted)"
              list="function-run-queues"
              onChange={(event) => setQueue(event.target.value)}
              placeholder="queue"
              value={queue}
            />
            <datalist id="function-run-queues">
              {queues.map((item) => (
                <option key={item} value={item}>
                  {item}
                </option>
              ))}
            </datalist>
          </label>
          <label className="ml-auto flex h-6 w-[min(380px,36vw)] items-center gap-2 border border-(--border-subtle) bg-(--elevated) px-2 font-mono text-(--muted)">
            <Search size={12} />
            <input
              aria-label="Search functions"
              className="w-full bg-transparent text-[10px] text-(--foreground) outline-hidden placeholder:text-(--muted)"
              onChange={(event) => setQuery(event.target.value)}
              placeholder="function / id / schema / correlation"
              value={query}
            />
          </label>
        </div>

        <div className="min-h-0 overflow-auto">
          <div className="grid h-7 grid-cols-[94px_minmax(240px,1.35fr)_minmax(150px,0.8fr)_minmax(132px,0.7fr)_86px_160px_88px] items-center gap-3 border-b border-(--border-subtle) bg-[color-mix(in_srgb,var(--elevated)_52%,transparent)] px-3 font-mono text-[9px] uppercase tracking-[0.08em] text-(--muted)">
            <span>status</span>
            <span>function</span>
            <span>module</span>
            <span>queue</span>
            <span>attempts</span>
            <span>correlation</span>
            <span>created</span>
          </div>
          {functionsQuery.isLoading ? (
            <OperationsLoadingRows />
          ) : functionsQuery.isError ? (
            <OperationsMessageRow
              message={errorMessage(functionsQuery.error)}
              tone="error"
            />
          ) : visible.length === 0 ? (
            <OperationsMessageRow message="no function runs matched" />
          ) : (
            visible.map((run) => {
              const isSelected = selected?.id === run.id;
              return (
                <button
                  className={cn(
                    "grid min-h-14 w-full grid-cols-[94px_minmax(240px,1.35fr)_minmax(150px,0.8fr)_minmax(132px,0.7fr)_86px_160px_88px] items-center gap-3 border-b border-(--border-subtle) px-3 text-left font-mono text-[11px]",
                    isSelected
                      ? "bg-(--accent-soft) shadow-[inset_2px_0_0_var(--accent)]"
                      : "hover:bg-(--elevated)"
                  )}
                  key={run.id}
                  onClick={() => {
                    pushFunctionUrl({ selectedId: run.id });
                    setSelectedId(run.id);
                  }}
                  type="button"
                >
                  <FunctionStatusPill status={run.status} />
                  <span className="min-w-0">
                    <span className="block truncate text-(--foreground)">
                      {run.functionName}
                    </span>
                    <span className="block truncate text-[10px] text-(--muted)">
                      {run.id}
                    </span>
                  </span>
                  <span className="min-w-0">
                    <span className="block truncate text-(--foreground)">
                      {run.runtimeDeclaration?.moduleName ?? "-"}
                    </span>
                    <span className="block truncate text-[10px] text-(--muted)">
                      {run.runtimeDeclaration?.moduleSource ?? "undeclared"}
                    </span>
                  </span>
                  <span className="min-w-0">
                    <span className="block truncate text-(--foreground)">
                      {run.runtimeDeclaration?.queue ?? "-"}
                    </span>
                    <span className="block truncate text-[10px] text-(--muted)">
                      {run.runtimeDeclaration?.inputSchema ?? "-"}
                    </span>
                  </span>
                  <span className="text-(--secondary)">
                    {run.attempts}/{run.maxAttempts}
                  </span>
                  <span className="truncate text-[10px] text-(--muted)">
                    {run.correlationId}
                  </span>
                  <span className="text-right text-[10px] text-(--muted)">
                    {time(run.createdAt)}
                  </span>
                </button>
              );
            })
          )}
        </div>
      </main>

      <ResizeHandle
        ariaLabel="Resize function inspector panel"
        onReset={resetLayout}
        onResize={resizeInspector}
      />

      <aside className="relative z-0 grid min-h-0 min-w-0 grid-rows-[auto_minmax(0,1fr)_auto] overflow-hidden bg-(--sidebar)">
        <InspectorHeader run={selected} />
        <div className="min-h-0 overflow-auto">
          {selected ? (
            <FunctionInspector run={selected} />
          ) : (
            <OperationsMessageRow message="select a function run" />
          )}
        </div>
        <div className="flex gap-2 border-t border-(--border-subtle) bg-(--surface) p-2">
          <Button
            disabled={!selected}
            onClick={() =>
              selected &&
              openStoryTarget({
                correlationId: selected.correlationId,
                nodeIdCandidates: [selected.id],
              })
            }
            variant="ghost"
          >
            <ExternalLink size={13} />
            Story
          </Button>
          <Button
            disabled={!selected}
            onClick={() => selected && retryRun(selected)}
            variant="danger"
          >
            <RotateCcw size={13} />
            Retry
          </Button>
          <Button
            disabled={functionsQuery.isRefetching}
            onClick={() => functionsQuery.refetch()}
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
  rows: FunctionRunAggregate[];
  title: string;
}) {
  return (
    <section className="min-w-0 border-r border-(--border-subtle) last:border-r-0">
      <div className="grid h-7 grid-cols-[minmax(0,1fr)_52px_52px_72px] items-center gap-2 border-b border-(--border-subtle) bg-[color-mix(in_srgb,var(--elevated)_52%,transparent)] px-3 font-mono text-[9px] uppercase tracking-[0.08em] text-(--muted)">
        <span>{title}</span>
        <span>fail</span>
        <span>dead</span>
        <span>avg</span>
      </div>
      <div>
        {rows.length === 0 ? (
          <div className="px-3 py-2 font-mono text-[10px] text-(--muted)">
            empty
          </div>
        ) : (
          rows.map((row) => (
            <button
              className="grid h-8 w-full grid-cols-[minmax(0,1fr)_52px_52px_72px] items-center gap-2 border-b border-(--border-subtle) px-3 text-left font-mono text-[10px] hover:bg-(--elevated)"
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
              <span
                className={row.dead > 0 ? "text-[#ef4444]" : "text-(--muted)"}
              >
                {row.dead}
              </span>
              <span className="text-(--muted)">
                {formatFunctionDuration(row.avgDurationMs)}
              </span>
            </button>
          ))
        )}
      </div>
    </section>
  );
}

function InspectorHeader({ run }: { run: FunctionRun | null }) {
  return (
    <header className="border-b border-(--border-subtle) bg-(--surface) px-3 py-2 font-mono">
      <div className="mb-1 text-[9px] font-semibold uppercase tracking-[0.12em] text-(--accent)">
        {run?.runtimeDeclaration?.moduleName ?? "Function"}
      </div>
      <div className="truncate text-[13px] font-semibold text-(--foreground)">
        {run ? run.functionName : "No run selected"}
      </div>
      {run ? (
        <div className="mt-1 flex items-center gap-2 text-[10px] text-(--muted)">
          <span className="truncate">{run.id}</span>
          <span>{formatFunctionDuration(runDurationMs(run))}</span>
          <span>{run.status}</span>
        </div>
      ) : null}
    </header>
  );
}

function FunctionStatusPill({ status }: { status: FunctionRun["status"] }) {
  return (
    <span
      className={cn(
        "inline-flex h-5 w-[76px] items-center justify-center border px-1.5 font-mono text-[10px] font-semibold",
        status === "completed" &&
          "border-[color-mix(in_srgb,#22c55e_34%,transparent)] bg-[color-mix(in_srgb,#22c55e_10%,transparent)] text-[#22c55e]",
        (status === "pending" ||
          status === "processing" ||
          status === "published" ||
          status === "running") &&
          "border-[color-mix(in_srgb,var(--accent)_34%,transparent)] bg-[color-mix(in_srgb,var(--accent)_10%,transparent)] text-(--accent)",
        status === "failed" &&
          "border-[color-mix(in_srgb,#f59e0b_34%,transparent)] bg-[color-mix(in_srgb,#f59e0b_10%,transparent)] text-[#f59e0b]",
        status === "dead" &&
          "border-[color-mix(in_srgb,var(--error)_35%,transparent)] bg-[color-mix(in_srgb,var(--error)_10%,transparent)] text-[#ef4444]"
      )}
    >
      {status}
    </span>
  );
}

function FunctionInspector({ run }: { run: FunctionRun }) {
  const detailQuery = useRuntimeFunctionDetail(run);
  const displayRun = detailQuery.data ?? run;
  return (
    <div className="grid">
      {detailQuery.isFetching ? (
        <OperationsMessageRow message="loading detail" />
      ) : detailQuery.isError ? (
        <OperationsMessageRow
          message={errorMessage(detailQuery.error)}
          tone="error"
        />
      ) : null}
      <KeyValueRows
        rows={[
          ["status", displayRun.status],
          ["function", displayRun.functionName],
          ["id", displayRun.id],
          ["module", displayRun.runtimeDeclaration?.moduleName ?? "-"],
          ["source", displayRun.runtimeDeclaration?.moduleSource ?? "-"],
          ["queue", displayRun.runtimeDeclaration?.queue ?? "-"],
          ["schema", displayRun.runtimeDeclaration?.inputSchema ?? "-"],
          ["version", String(displayRun.runtimeDeclaration?.version ?? "-")],
          ["attempts", `${displayRun.attempts}/${displayRun.maxAttempts}`],
          ["duration", formatFunctionDuration(runDurationMs(displayRun))],
          ["locked_by", displayRun.lockedBy ?? "-"],
          ["correlation", displayRun.correlationId],
          ["actor", actorLabel(displayRun.actor)],
          ["created", displayRun.createdAt],
          ["started", displayRun.startedAt ?? "-"],
          ["completed", displayRun.completedAt ?? "-"],
          ["error", displayRun.lastError ?? "-"],
        ]}
      />
      <JsonViewer defaultExpanded title="input" value={displayRun.input} />
      {displayRun.output ? (
        <JsonViewer title="output" value={displayRun.output} />
      ) : null}
      {displayRun.runtimeDeclaration?.retryPolicy ? (
        <JsonViewer
          title="retry policy"
          value={displayRun.runtimeDeclaration.retryPolicy}
        />
      ) : null}
      <JsonViewer title="logs" value={displayRun.logs} />
    </div>
  );
}

function KeyValueRows({ rows }: { rows: Array<[string, string]> }) {
  return (
    <div className="w-max min-w-full border-b border-(--border-subtle) font-mono text-xs">
      {rows.map(([key, value]) => (
        <div
          className="grid w-max min-w-full grid-cols-[124px_minmax(220px,max-content)] border-b border-(--border-subtle) last:border-b-0"
          key={key}
        >
          <div className="bg-(--sidebar) px-3 py-1.5 text-(--muted)">{key}</div>
          <div className="whitespace-pre-wrap px-3 py-1.5 text-(--secondary)">
            {value}
          </div>
        </div>
      ))}
    </div>
  );
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : "Function runs unavailable";
}

function indexOf(items: FunctionRun[], id: string) {
  return Math.max(
    0,
    items.findIndex((item) => item.id === id)
  );
}

function readFunctionStatus(value: string): FunctionStatusFilter {
  return functionStatusFilters.includes(value as FunctionStatusFilter)
    ? (value as FunctionStatusFilter)
    : "all";
}
