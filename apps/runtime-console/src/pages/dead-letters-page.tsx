import { RotateCcw, Search } from "lucide-react";
import { useEffect, useMemo, useState } from "react";

import { useRuntimeConsole } from "../components/runtime/runtime-console-context";
import { StatusPill } from "../components/runtime/status-pill";
import { Button } from "../components/ui/button";
import { EmptyState as EmptyStateView } from "../components/ui/empty-state";
import { Panel } from "../components/ui/panel";
import {
  retryTargetFor,
  type FunctionRun,
  type RuntimeEvent,
} from "../data/mock-runtime";
import { useListKeyboard } from "../hooks/use-list-keyboard";
import { useDeadLetters } from "../hooks/use-runtime-queries";
import { time } from "../lib/format";

type DeadLetter =
  | { kind: "event"; item: RuntimeEvent }
  | { kind: "function"; item: FunctionRun };

export function DeadLettersPage() {
  const { openDrawer, openRetry, openTimeline } = useRuntimeConsole();
  const [query, setQuery] = useState("");
  const [kind, setKind] = useState<"all" | "event" | "function">("all");
  const [oldestFirst, setOldestFirst] = useState(true);
  const deadLettersQuery = useDeadLetters();
  const failures = useMemo<DeadLetter[]>(
    () => deadLettersQuery.data ?? [],
    [deadLettersQuery.data]
  );

  const visible = failures
    .filter((failure) => {
      const name =
        failure.kind === "event"
          ? failure.item.eventName
          : failure.item.functionName;
      const matchesKind = kind === "all" || failure.kind === kind;
      const text =
        `${name} ${failure.item.id} ${failure.item.correlationId}`.toLowerCase();
      return matchesKind && text.includes(query.trim().toLowerCase());
    })
    .sort((a, b) =>
      oldestFirst
        ? a.item.createdAt.localeCompare(b.item.createdAt)
        : b.item.createdAt.localeCompare(a.item.createdAt)
    );

  const [selectedIndex, setSelectedIndex] = useState(0);
  useEffect(() => {
    setSelectedIndex(0);
  }, [kind, oldestFirst, query]);

  const selected = visible[selectedIndex] ?? null;
  const openFailure = (failure: DeadLetter) =>
    openDrawer(
      failure.kind === "event"
        ? { kind: "event", item: failure.item }
        : { kind: "function", item: failure.item }
    );
  const retryFailure = (failure: DeadLetter) => {
    const retryTarget = retryTargetFor(
      failure.kind === "event"
        ? { kind: "event", item: failure.item }
        : { kind: "function", item: failure.item }
    );
    if (retryTarget) {
      openRetry(retryTarget);
    }
  };

  useListKeyboard({
    items: visible,
    selectedIndex,
    setSelectedIndex,
    onOpen: openFailure,
    onRetry: retryFailure,
  });

  return (
    <section>
      <div className="mb-5 flex items-end justify-between gap-6">
        <div>
          <h1 className="text-2xl font-semibold text-slate-100">
            Dead Letters
          </h1>
          <p className="mt-1.5 max-w-2xl text-[13px] leading-6 text-slate-400">
            Failure inbox for retryable and exhausted runtime work.
          </p>
        </div>
      </div>

      <div className="mb-3 flex flex-wrap items-center gap-2.5">
        <Button
          onClick={() => setKind("all")}
          variant={kind === "all" ? "default" : "ghost"}
        >
          All
        </Button>
        <Button
          onClick={() => setKind("event")}
          variant={kind === "event" ? "default" : "ghost"}
        >
          Events
        </Button>
        <Button
          onClick={() => setKind("function")}
          variant={kind === "function" ? "default" : "ghost"}
        >
          Functions
        </Button>
        <Button
          onClick={() => setOldestFirst((current) => !current)}
          variant="ghost"
        >
          {oldestFirst ? "Oldest first" : "Newest first"}
        </Button>
        <label className="flex h-9 min-w-[min(420px,100%)] items-center gap-2.5 rounded-lg border border-white/10 bg-white/[0.035] px-3 text-slate-400">
          <Search size={15} />
          <input
            aria-label="Search dead letters"
            className="w-full bg-transparent text-[13px] text-slate-100 outline-none placeholder:text-slate-600"
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Search failure, id, correlation..."
            value={query}
          />
        </label>
      </div>

      <Panel>
        {deadLettersQuery.isLoading ? (
          <>
            <div className="h-20 animate-pulse border-b border-white/10 bg-white/[0.03]" />
            <div className="h-20 animate-pulse border-b border-white/10 bg-white/[0.03]" />
            <div className="h-20 animate-pulse bg-white/[0.03]" />
          </>
        ) : deadLettersQuery.isError ? (
          <div className="m-3 rounded-lg border border-rose-300/30 bg-black/20 p-3 text-xs text-rose-100">
            {errorMessage(deadLettersQuery.error)}
          </div>
        ) : visible.length === 0 ? (
          <EmptyStateView>
            <EmptyStateView.Title>
              No failed or dead runtime work.
            </EmptyStateView.Title>
            <EmptyStateView.Description>
              The failure inbox is clear.
            </EmptyStateView.Description>
          </EmptyStateView>
        ) : (
          visible.map((failure) => {
            const { item } = failure;
            const name =
              failure.kind === "event"
                ? failure.item.eventName
                : failure.item.functionName;
            return (
              <div
                className={`grid w-full grid-cols-[108px_minmax(0,1fr)_auto] items-center gap-3.5 border-b border-white/10 bg-transparent p-3.5 text-left text-slate-100 last:border-b-0 hover:bg-blue-300/[0.055] max-md:grid-cols-1 ${
                  selected?.item.id === item.id ? "bg-blue-300/[0.055]" : ""
                }`}
                key={item.id}
              >
                <StatusPill status={item.status} />
                <button
                  className="min-w-0 bg-transparent text-left text-slate-100"
                  onClick={(event) => {
                    event.stopPropagation();
                    setSelectedIndex(indexOf(visible, item.id));
                    openFailure(failure);
                  }}
                >
                  <div className="truncate text-[13px] font-semibold text-slate-100">
                    {name}
                  </div>
                  <div className="mono mt-0.5 truncate text-xs text-slate-500">
                    {failure.kind} · {item.id} · {item.correlationId} ·{" "}
                    {time(item.createdAt)}
                  </div>
                  {item.lastError ? (
                    <div className="mono mt-2 text-xs text-rose-100">
                      {item.lastError}
                    </div>
                  ) : null}
                </button>
                <div className="flex gap-2.5">
                  <Button
                    onClick={(event) => {
                      event.stopPropagation();
                      openTimeline(item.correlationId);
                    }}
                    variant="ghost"
                  >
                    Timeline
                  </Button>
                  <Button
                    onClick={(event) => {
                      event.stopPropagation();
                      retryFailure(failure);
                    }}
                    variant="danger"
                  >
                    <RotateCcw size={15} />
                    Retry
                  </Button>
                </div>
              </div>
            );
          })
        )}
      </Panel>
    </section>
  );
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : "Runtime request failed";
}

function indexOf(items: DeadLetter[], id: string) {
  return Math.max(
    0,
    items.findIndex((item) => item.item.id === id)
  );
}
