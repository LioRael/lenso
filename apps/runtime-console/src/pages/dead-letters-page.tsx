import {
  ExternalLink,
  RefreshCcw,
  RotateCcw,
  Search,
  TriangleAlert,
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";

import { JsonViewer } from "../components/runtime/json-viewer";
import { ResizeHandle } from "../components/runtime/resize-handle";
import { useRuntimeConsole } from "../components/runtime/runtime-console-context";
import { StatusPill } from "../components/runtime/status-pill";
import { Button } from "../components/ui/button";
import {
  retryTargetFor,
  type FunctionRun,
  type RuntimeEvent,
} from "../data/mock-runtime";
import { useBrowserUrlPopState } from "../hooks/use-browser-url-state";
import { useListKeyboard } from "../hooks/use-list-keyboard";
import { usePersistedLayout } from "../hooks/use-persisted-layout";
import {
  useDeadLetters,
  useRuntimeEventDetail,
  useRuntimeFunctionDetail,
} from "../hooks/use-runtime-queries";
import { actorLabel, time } from "../lib/format";
import { runtimeConsoleDataSource } from "../lib/http-client";
import {
  resizeOperationsInspectorWidth,
  type OperationsInspectorLayout,
} from "./operations-layout";
import {
  deadLettersPath,
  pushOperationsUrl,
  readOperationsParam,
  replaceOperationsUrl,
} from "./operations-url-model";

type DeadLetter =
  | { kind: "event"; item: RuntimeEvent }
  | { kind: "function"; item: FunctionRun };

const deadLettersLayoutDefaults = {
  inspectorWidth: 376,
} satisfies OperationsInspectorLayout;

export function DeadLettersPage() {
  const { openRetry, openStoryTarget } = useRuntimeConsole();
  const [query, setQuery] = useState(() => readOperationsParam("q"));
  const [kind, setKind] = useState<"all" | "event" | "function">(() =>
    readDeadLetterKind(readOperationsParam("kind"))
  );
  const [oldestFirst, setOldestFirst] = useState(
    () => readOperationsParam("order") !== "newest"
  );
  const [selectedId, setSelectedId] = useState(() =>
    readOperationsParam("selected")
  );
  const [layout, setLayout, resetLayout] = usePersistedLayout(
    "runtime-console:dead-letters-layout",
    deadLettersLayoutDefaults
  );
  const deadLettersLayout = { ...deadLettersLayoutDefaults, ...layout };
  const deadLettersQuery = useDeadLetters();
  const failures = useMemo<DeadLetter[]>(
    () => deadLettersQuery.data ?? [],
    [deadLettersQuery.data]
  );

  const visible = useMemo(
    () =>
      failures
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
        ),
    [failures, kind, oldestFirst, query]
  );

  useBrowserUrlPopState((search) => {
    setQuery(search.get("q") ?? "");
    setKind(readDeadLetterKind(search.get("kind") ?? ""));
    setOldestFirst((search.get("order") ?? "oldest") !== "newest");
    setSelectedId(search.get("selected") ?? "");
  });

  const deadLetterUrl = (
    overrides: Partial<{
      kind: "all" | "event" | "function";
      oldestFirst: boolean;
      query: string;
      selectedId: string;
    }> = {}
  ) =>
    deadLettersPath({
      kind: overrides.kind ?? kind,
      oldestFirst: overrides.oldestFirst ?? oldestFirst,
      query: overrides.query ?? query,
      selectedId: overrides.selectedId ?? selectedId,
    });

  const pushDeadLetterUrl = (
    overrides: Parameters<typeof deadLetterUrl>[0] = {}
  ) => pushOperationsUrl(deadLetterUrl(overrides));

  useEffect(() => {
    if (visible.length === 0) {
      if (selectedId) {
        setSelectedId("");
      }
      return;
    }
    if (!visible.some((failure) => failure.item.id === selectedId)) {
      setSelectedId(visible[0]?.item.id ?? "");
    }
  }, [selectedId, visible]);

  useEffect(() => {
    replaceOperationsUrl(
      deadLettersPath({ kind, oldestFirst, query, selectedId })
    );
  }, [kind, oldestFirst, query, selectedId]);

  const selected =
    visible.find((failure) => failure.item.id === selectedId) ?? null;
  const selectedIndex = selected ? indexOf(visible, selected.item.id) : 0;
  const selectIndex = (index: number) => {
    const failure = visible[index];
    if (failure) {
      pushDeadLetterUrl({ selectedId: failure.item.id });
      setSelectedId(failure.item.id);
    }
  };
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

  const resizeInspector = (deltaX: number) => {
    setLayout((current) => ({
      ...current,
      inspectorWidth: resizeOperationsInspectorWidth({
        currentWidth: current.inspectorWidth,
        defaultWidth: deadLettersLayoutDefaults.inspectorWidth,
        deltaX,
        maxWidth: 560,
        minWidth: 320,
      }),
    }));
  };

  useListKeyboard({
    items: visible,
    selectedIndex,
    setSelectedIndex: selectIndex,
    onOpen: (failure) => {
      pushDeadLetterUrl({ selectedId: failure.item.id });
      setSelectedId(failure.item.id);
    },
    onRetry: retryFailure,
  });

  return (
    <section
      className="grid h-full min-h-0 min-w-0 overflow-hidden bg-(--background) text-(--foreground)"
      style={{
        gridTemplateColumns: `minmax(0,1fr) 1px ${deadLettersLayout.inspectorWidth}px`,
      }}
    >
      <main className="grid min-h-0 min-w-0 grid-rows-[auto_auto_minmax(0,1fr)] overflow-hidden border-r border-(--border-subtle)">
        <header className="border-b border-(--border-subtle) bg-(--surface) px-3 py-2">
          <div className="flex items-center gap-2">
            <TriangleAlert className="text-(--accent)" size={14} />
            <h1 className="font-mono text-[13px] font-semibold">
              Dead Letters
            </h1>
            <span className="ml-auto font-mono text-[10px] text-(--muted)">
              {visible.length} failures / {runtimeConsoleDataSource()}
            </span>
          </div>
        </header>

        <div className="flex h-9 items-center gap-2 border-b border-(--border-subtle) bg-(--background) px-3">
          {(["all", "event", "function"] as const).map((item) => (
            <button
              className={`h-6 border px-2 font-mono text-[10px] ${
                kind === item
                  ? "border-[color-mix(in_srgb,var(--accent)_40%,transparent)] bg-(--accent-soft) text-(--accent)"
                  : "border-(--border-subtle) text-(--muted) hover:text-(--foreground)"
              }`}
              key={item}
              onClick={() => {
                pushDeadLetterUrl({ kind: item, selectedId: "" });
                setKind(item);
              }}
              type="button"
            >
              {deadLetterKindLabel(item)}
            </button>
          ))}
          <button
            className="h-6 border border-(--border-subtle) px-2 font-mono text-[10px] text-(--muted) hover:text-(--foreground)"
            onClick={() => {
              const next = !oldestFirst;
              pushDeadLetterUrl({ oldestFirst: next, selectedId: "" });
              setOldestFirst(next);
            }}
            type="button"
          >
            {oldestFirst ? "oldest" : "newest"}
          </button>
          <label className="ml-auto flex h-6 w-[min(360px,45vw)] items-center gap-2 border border-(--border-subtle) bg-(--elevated) px-2 font-mono text-(--muted)">
            <Search size={12} />
            <input
              aria-label="Search dead letters"
              className="w-full bg-transparent text-[10px] text-(--foreground) outline-hidden placeholder:text-(--muted)"
              onChange={(event) => setQuery(event.target.value)}
              placeholder="failure / id / correlation"
              value={query}
            />
          </label>
        </div>

        <div className="min-h-0 overflow-auto">
          {deadLettersQuery.isLoading ? (
            <LoadingRows />
          ) : deadLettersQuery.isError ? (
            <MessageRow
              message={errorMessage(deadLettersQuery.error)}
              tone="error"
            />
          ) : visible.length === 0 ? (
            <MessageRow message="failure inbox clear" />
          ) : (
            visible.map((failure) => {
              const { item } = failure;
              const name =
                failure.kind === "event"
                  ? failure.item.eventName
                  : failure.item.functionName;
              const isSelected = selected?.item.id === item.id;
              return (
                <button
                  className={`grid w-full grid-cols-[104px_minmax(0,1fr)_116px] items-center gap-3 border-b border-(--border-subtle) px-3 py-2 text-left font-mono text-[11px] ${
                    isSelected
                      ? "bg-(--accent-soft) shadow-[inset_2px_0_0_var(--accent)]"
                      : "hover:bg-(--elevated)"
                  }`}
                  key={item.id}
                  onClick={() => {
                    pushDeadLetterUrl({ selectedId: item.id });
                    setSelectedId(item.id);
                  }}
                  type="button"
                >
                  <StatusPill status={item.status} />
                  <span className="min-w-0">
                    <span className="block truncate text-(--foreground)">
                      {name}
                    </span>
                    <span className="block truncate text-[10px] text-(--muted)">
                      {deadLetterKindLabel(failure.kind)} / {item.id} /{" "}
                      {item.correlationId}
                    </span>
                    {item.lastError ? (
                      <span className="mt-1 block truncate text-[10px] text-[#ef4444]">
                        {item.lastError}
                      </span>
                    ) : null}
                  </span>
                  <span className="text-right text-[10px] text-(--muted)">
                    {time(item.createdAt)}
                  </span>
                </button>
              );
            })
          )}
        </div>
      </main>

      <ResizeHandle
        ariaLabel="Resize failure inspector panel"
        onReset={resetLayout}
        onResize={resizeInspector}
      />

      <aside className="relative z-0 grid min-h-0 min-w-0 grid-rows-[auto_minmax(0,1fr)_auto] overflow-hidden bg-(--sidebar)">
        <InspectorHeader failure={selected} />
        <div className="min-h-0 overflow-auto">
          {selected ? (
            <FailureInspector failure={selected} />
          ) : (
            <MessageRow message="select a failed item" />
          )}
        </div>
        <div className="flex gap-2 border-t border-(--border-subtle) bg-(--surface) p-2">
          <Button
            disabled={!selected}
            onClick={() =>
              selected &&
              openStoryTarget({
                correlationId: selected.item.correlationId,
                nodeIdCandidates: [selected.item.id],
              })
            }
            variant="ghost"
          >
            <ExternalLink size={13} />
            Story
          </Button>
          <Button
            disabled={!selected}
            onClick={() => selected && retryFailure(selected)}
            variant="danger"
          >
            <RotateCcw size={13} />
            Retry
          </Button>
          <Button
            disabled={deadLettersQuery.isRefetching}
            onClick={() => deadLettersQuery.refetch()}
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

function InspectorHeader({ failure }: { failure: DeadLetter | null }) {
  const name = failure
    ? failure.kind === "event"
      ? failure.item.eventName
      : failure.item.functionName
    : "No failure selected";
  const kind = failure ? deadLetterKindLabel(failure.kind) : "Failure";
  return (
    <header className="border-b border-(--border-subtle) bg-(--surface) px-3 py-2 font-mono">
      <div className="mb-1 text-[9px] font-semibold uppercase tracking-[0.12em] text-(--accent)">
        {kind}
      </div>
      <div className="truncate text-[13px] font-semibold text-(--foreground)">
        {name}
      </div>
      {failure ? (
        <div className="mt-1 flex items-center gap-2 text-[10px] text-(--muted)">
          <span className="truncate">{failure.item.id}</span>
          <span>
            {failure.item.attempts}/{failure.item.maxAttempts}
          </span>
          <span>{failure.item.status}</span>
        </div>
      ) : null}
    </header>
  );
}

function FailureInspector({ failure }: { failure: DeadLetter }) {
  return failure.kind === "event" ? (
    <EventFailureInspector event={failure.item} />
  ) : (
    <FunctionFailureInspector run={failure.item} />
  );
}

function EventFailureInspector({ event }: { event: RuntimeEvent }) {
  const detailQuery = useRuntimeEventDetail(event);
  const displayEvent = detailQuery.data ?? event;
  return (
    <div className="grid">
      {detailQuery.isFetching ? <MessageRow message="loading detail" /> : null}
      {detailQuery.isError ? (
        <MessageRow message={errorMessage(detailQuery.error)} tone="error" />
      ) : null}
      <KeyValueRows
        rows={[
          ["status", displayEvent.status],
          ["outbox_event", displayEvent.eventName],
          ["id", displayEvent.id],
          [
            "aggregate",
            `${displayEvent.aggregateType}:${displayEvent.aggregateId}`,
          ],
          ["source", displayEvent.sourceModule ?? "-"],
          ["version", String(displayEvent.eventVersion ?? "-")],
          ["attempts", `${displayEvent.attempts}/${displayEvent.maxAttempts}`],
          ["correlation", displayEvent.correlationId],
          ["causation", displayEvent.causationId],
          ["actor", actorLabel(displayEvent.actor)],
          ["created", displayEvent.createdAt],
          ["occurred", displayEvent.occurredAt ?? "-"],
          ["published", displayEvent.publishedAt ?? "-"],
          ["last_error", displayEvent.lastError ?? "-"],
        ]}
      />
      <JsonViewer
        defaultExpanded
        title="outbox event payload"
        value={displayEvent.payload}
      />
      {displayEvent.headers ? (
        <JsonViewer title="headers" value={displayEvent.headers} />
      ) : null}
      {displayEvent.trace ? (
        <JsonViewer title="trace" value={displayEvent.trace} />
      ) : null}
    </div>
  );
}

function FunctionFailureInspector({ run }: { run: FunctionRun }) {
  const detailQuery = useRuntimeFunctionDetail(run);
  const displayRun = detailQuery.data ?? run;
  const declaration = displayRun.runtimeDeclaration;
  return (
    <div className="grid">
      {detailQuery.isFetching ? <MessageRow message="loading detail" /> : null}
      {detailQuery.isError ? (
        <MessageRow message={errorMessage(detailQuery.error)} tone="error" />
      ) : null}
      <KeyValueRows
        rows={[
          ["status", displayRun.status],
          ["function", displayRun.functionName],
          ["id", displayRun.id],
          ["module", declaration?.moduleName ?? "-"],
          ["source", declaration?.moduleSource ?? "-"],
          ["queue", declaration?.queue ?? "-"],
          ["schema", declaration?.inputSchema ?? "-"],
          ["attempts", `${displayRun.attempts}/${displayRun.maxAttempts}`],
          ["correlation", displayRun.correlationId],
          ["actor", actorLabel(displayRun.actor)],
          ["created", displayRun.createdAt],
          ["started", displayRun.startedAt ?? "-"],
          ["completed", displayRun.completedAt ?? "-"],
          ["last_error", displayRun.lastError ?? "-"],
        ]}
      />
      <JsonViewer
        defaultExpanded
        title="function input"
        value={displayRun.input}
      />
      {displayRun.output ? (
        <JsonViewer title="function output" value={displayRun.output} />
      ) : null}
      {declaration?.retryPolicy ? (
        <JsonViewer title="retry policy" value={declaration.retryPolicy} />
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
      className={`border-b border-(--border-subtle) px-3 py-3 font-mono text-[11px] ${
        tone === "error" ? "text-[#ef4444]" : "text-(--muted)"
      }`}
    >
      {message}
    </div>
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

function readDeadLetterKind(value: string): "all" | "event" | "function" {
  return value === "event" || value === "function" ? value : "all";
}

function deadLetterKindLabel(kind: DeadLetter["kind"] | "all") {
  if (kind === "event") {
    return "outbox";
  }
  if (kind === "function") {
    return "functions";
  }
  return "all";
}
