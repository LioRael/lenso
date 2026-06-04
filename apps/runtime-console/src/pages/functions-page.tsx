import { Search } from "lucide-react";
import { useEffect, useMemo, useState } from "react";

import { useRuntimeConsole } from "../components/runtime/runtime-console-context";
import { StatusPill } from "../components/runtime/status-pill";
import { Button } from "../components/ui/button";
import { EmptyState as EmptyStateView } from "../components/ui/empty-state";
import { Panel } from "../components/ui/panel";
import {
  retryTargetFor,
  type FunctionRun,
  type RuntimeStatus,
} from "../data/mock-runtime";
import { useListKeyboard } from "../hooks/use-list-keyboard";
import { useRuntimeFunctions } from "../hooks/use-runtime-queries";
import { duration, time } from "../lib/format";

const statuses: Array<RuntimeStatus | "all"> = [
  "all",
  "pending",
  "running",
  "completed",
  "failed",
  "dead",
];

export function FunctionsPage() {
  const { openDrawer, openRetry } = useRuntimeConsole();
  const [query, setQuery] = useState("");
  const [status, setStatus] = useState<RuntimeStatus | "all">("all");
  const [selectedIndex, setSelectedIndex] = useState(0);
  const functionsQuery = useRuntimeFunctions();

  const filtered = useMemo(() => {
    const normalized = query.trim().toLowerCase();
    return (functionsQuery.data ?? []).filter((run) => {
      const matchesQuery =
        normalized.length === 0 ||
        run.functionName.toLowerCase().includes(normalized) ||
        (run.runtimeDeclaration?.moduleName ?? "")
          .toLowerCase()
          .includes(normalized) ||
        (run.runtimeDeclaration?.moduleSource ?? "")
          .toLowerCase()
          .includes(normalized) ||
        (run.runtimeDeclaration?.queue ?? "")
          .toLowerCase()
          .includes(normalized) ||
        (run.runtimeDeclaration?.inputSchema ?? "")
          .toLowerCase()
          .includes(normalized) ||
        run.correlationId.toLowerCase().includes(normalized) ||
        run.id.toLowerCase().includes(normalized);
      const matchesStatus = status === "all" || run.status === status;
      return matchesQuery && matchesStatus;
    });
  }, [functionsQuery.data, query, status]);

  useEffect(() => {
    setSelectedIndex(0);
  }, [query, status]);

  const selected = filtered[selectedIndex] ?? null;
  const openRun = (run: FunctionRun) =>
    openDrawer({ kind: "function", item: run });
  const retryRun = (run: FunctionRun) => {
    const retryTarget = retryTargetFor({ kind: "function", item: run });
    if (retryTarget) {
      openRetry(retryTarget);
    }
  };

  useListKeyboard({
    items: filtered,
    selectedIndex,
    setSelectedIndex,
    onOpen: openRun,
    onRetry: retryRun,
  });

  return (
    <section>
      <div className="mb-5 flex items-end justify-between gap-6">
        <div>
          <h1 className="text-2xl font-semibold text-(--foreground)">
            Functions
          </h1>
          <p className="mt-1.5 max-w-2xl text-[13px] leading-6 text-(--secondary)">
            Inspect runtime function runs, attempts, duration, logs, and retry
            state.
          </p>
        </div>
      </div>

      <div className="mb-3 flex flex-wrap items-center gap-2.5">
        <label className="flex h-9 min-w-[min(420px,100%)] items-center gap-2.5 rounded-lg border border-(--border-subtle) bg-(--elevated) px-3 text-(--secondary)">
          <Search size={15} />
          <input
            aria-label="Search functions"
            className="w-full bg-transparent text-[13px] text-(--foreground) outline-hidden placeholder:text-(--muted)"
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Search function, module, queue, schema, correlation..."
            value={query}
          />
        </label>
        <select
          aria-label="Filter function status"
          className="h-9 rounded-lg border border-(--border-subtle) bg-(--elevated) px-3 text-[13px] text-(--foreground) outline-hidden"
          onChange={(event) =>
            setStatus(event.target.value as RuntimeStatus | "all")
          }
          value={status}
        >
          {statuses.map((item) => (
            <option key={item} value={item}>
              {item}
            </option>
          ))}
        </select>
        <Button onClick={() => setQuery("")} variant="ghost">
          Reset
        </Button>
      </div>

      <Panel>
        <div className="grid grid-cols-[104px_minmax(220px,1.35fr)_minmax(150px,0.8fr)_72px_112px_146px] border-b border-(--border-subtle) px-3.5 py-2.5 text-[11px] font-semibold uppercase tracking-[0.04em] text-(--muted) max-md:hidden">
          <span>status</span>
          <span>function</span>
          <span>module</span>
          <span>attempts</span>
          <span>duration</span>
          <span>correlation</span>
        </div>
        <div className="grid">
          {functionsQuery.isLoading ? (
            <LoadingRows />
          ) : functionsQuery.isError ? (
            <ErrorState message={errorMessage(functionsQuery.error)} />
          ) : filtered.length === 0 ? (
            <EmptyState label="No function runs matched this view" />
          ) : (
            filtered.map((run) => (
              <button
                className={`grid w-full grid-cols-[104px_minmax(220px,1.35fr)_minmax(150px,0.8fr)_72px_112px_146px] items-center gap-2.5 border-b border-(--border-subtle) bg-transparent px-3.5 py-3 text-left text-(--foreground) last:border-b-0 hover:bg-(--hover) max-md:grid-cols-1 ${
                  selected?.id === run.id ? "bg-(--hover)" : ""
                }`}
                key={run.id}
                onClick={() => {
                  setSelectedIndex(indexOf(filtered, run.id));
                  openRun(run);
                }}
              >
                <StatusPill status={run.status} />
                <div className="min-w-0">
                  <div className="truncate text-[13px] font-semibold text-(--foreground)">
                    {run.functionName}
                  </div>
                  <div className="mono mt-0.5 truncate text-xs text-(--muted)">
                    {run.runtimeDeclaration?.queue ?? run.id}
                  </div>
                </div>
                <div className="min-w-0 text-xs">
                  <div className="truncate font-semibold text-(--secondary)">
                    {run.runtimeDeclaration?.moduleName ?? "-"}
                  </div>
                  <div className="mt-0.5 flex min-w-0 items-center gap-1.5 text-[11px] text-(--muted)">
                    <span className="truncate">
                      {run.runtimeDeclaration?.moduleSource ?? "undeclared"}
                    </span>
                    {run.runtimeDeclaration?.inputSchema ? (
                      <>
                        <span>/</span>
                        <span className="truncate">
                          {run.runtimeDeclaration.inputSchema}
                        </span>
                      </>
                    ) : null}
                  </div>
                </div>
                <span className="mono">
                  {run.attempts}/{run.maxAttempts}
                </span>
                <span>{duration(run.startedAt, run.completedAt)}</span>
                <span className="mono truncate text-xs text-(--muted)">
                  {run.correlationId}
                </span>
              </button>
            ))
          )}
        </div>
      </Panel>

      <p className="mt-3 text-xs text-(--muted)">
        Selected run created at {selected ? time(selected.createdAt) : "—"}.
      </p>
    </section>
  );
}

function LoadingRows() {
  return (
    <>
      <div className="h-14 animate-pulse border-b border-(--border-subtle) bg-(--elevated)" />
      <div className="h-14 animate-pulse border-b border-(--border-subtle) bg-(--elevated)" />
      <div className="h-14 animate-pulse bg-(--elevated)" />
    </>
  );
}

function ErrorState({ message }: { message: string }) {
  return (
    <div className="m-3 rounded-lg border border-[color-mix(in_srgb,var(--error)_30%,transparent)] bg-[color-mix(in_srgb,var(--error)_8%,transparent)] p-3 text-xs text-(--error)">
      {message}
    </div>
  );
}

function EmptyState({ label }: { label: string }) {
  return (
    <EmptyStateView>
      <EmptyStateView.Title>{label}</EmptyStateView.Title>
      <EmptyStateView.Description>
        Try another status or search term.
      </EmptyStateView.Description>
    </EmptyStateView>
  );
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : "Runtime request failed";
}

function indexOf(items: FunctionRun[], id: string) {
  return Math.max(
    0,
    items.findIndex((item) => item.id === id)
  );
}
