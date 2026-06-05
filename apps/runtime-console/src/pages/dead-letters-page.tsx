import {
  ExternalLink,
  RefreshCcw,
  RotateCcw,
  TriangleAlert,
} from "lucide-react";
import { useMemo, useState } from "react";

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
import { useListKeyboard } from "../hooks/use-list-keyboard";
import {
  useDeadLetters,
  useRuntimeEventDetail,
  useRuntimeFunctionDetail,
} from "../hooks/use-runtime-queries";
import { actorLabel, time } from "../lib/format";
import { runtimeConsoleDataSource } from "../lib/http-client";
import {
  OperationsFilterBar,
  OperationsFilterChip,
  OperationsSearchInput,
} from "./operations-filter";
import { OperationsInspectorHeader } from "./operations-inspector";
import { useOperationsInspectorLayout } from "./operations-layout";
import { useOperationsSelection } from "./operations-selection";
import {
  OperationsLoadingRows,
  OperationsMessageRow,
} from "./operations-state";
import { OperationsKeyValueRows } from "./operations-table";
import { deadLettersPath, pushOperationsUrl } from "./operations-url-model";
import {
  readOperationsParamValue,
  useOperationsUrlPopState,
} from "./operations-url-state";

type DeadLetter =
  | { kind: "event"; item: RuntimeEvent }
  | { kind: "function"; item: FunctionRun };

export function DeadLettersPage() {
  const { openRetry, openStoryTarget } = useRuntimeConsole();
  const [query, setQuery] = useState(() => readOperationsParamValue("q"));
  const [kind, setKind] = useState<"all" | "event" | "function">(() =>
    readOperationsParamValue("kind", readDeadLetterKind)
  );
  const [oldestFirst, setOldestFirst] = useState(
    () => readOperationsParamValue("order") !== "newest"
  );
  const [selectedId, setSelectedId] = useState(() =>
    readOperationsParamValue("selected")
  );
  const { inspectorWidth, resetLayout, resizeInspector } =
    useOperationsInspectorLayout({
      defaultWidth: 376,
      maxWidth: 560,
      minWidth: 320,
      storageKey: "runtime-console:dead-letters-layout",
    });
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

  useOperationsUrlPopState([
    { name: "q", setValue: setQuery },
    { name: "kind", parse: readDeadLetterKind, setValue: setKind },
    {
      name: "order",
      parse: (value) => value !== "newest",
      setValue: setOldestFirst,
    },
    { name: "selected", setValue: setSelectedId },
  ]);

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

  const { selected, selectedIndex, selectIndex, selectItem } =
    useOperationsSelection({
      currentPath: deadLettersPath({ kind, oldestFirst, query, selectedId }),
      getId: (failure) => failure.item.id,
      items: visible,
      pathForSelectedId: (nextSelectedId) =>
        deadLetterUrl({ selectedId: nextSelectedId }),
      selectedId,
      setSelectedId,
    });
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
    setSelectedIndex: selectIndex,
    onOpen: selectItem,
    onRetry: retryFailure,
  });

  return (
    <section
      className="grid h-full min-h-0 min-w-0 overflow-hidden bg-(--background) text-(--foreground)"
      style={{
        gridTemplateColumns: `minmax(0,1fr) 1px ${inspectorWidth}px`,
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

        <OperationsFilterBar>
          {(["all", "event", "function"] as const).map((item) => (
            <OperationsFilterChip
              active={kind === item}
              key={item}
              onClick={() => {
                pushDeadLetterUrl({ kind: item, selectedId: "" });
                setKind(item);
              }}
            >
              {deadLetterKindLabel(item)}
            </OperationsFilterChip>
          ))}
          <OperationsFilterChip
            onClick={() => {
              const next = !oldestFirst;
              pushDeadLetterUrl({ oldestFirst: next, selectedId: "" });
              setOldestFirst(next);
            }}
          >
            {oldestFirst ? "oldest" : "newest"}
          </OperationsFilterChip>
          <OperationsSearchInput
            ariaLabel="Search dead letters"
            onChange={setQuery}
            placeholder="failure / id / correlation"
            value={query}
          />
        </OperationsFilterBar>

        <div className="min-h-0 overflow-auto">
          {deadLettersQuery.isLoading ? (
            <OperationsLoadingRows />
          ) : deadLettersQuery.isError ? (
            <OperationsMessageRow
              message={errorMessage(deadLettersQuery.error)}
              tone="error"
            />
          ) : visible.length === 0 ? (
            <OperationsMessageRow message="failure inbox clear" />
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
                  onClick={() => selectItem(failure)}
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
            <OperationsMessageRow message="select a failed item" />
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
    <OperationsInspectorHeader
      eyebrow={kind}
      meta={
        failure ? (
          <>
            <span className="truncate">{failure.item.id}</span>
            <span>
              {failure.item.attempts}/{failure.item.maxAttempts}
            </span>
            <span>{failure.item.status}</span>
          </>
        ) : null
      }
      title={name}
    />
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
      {detailQuery.isFetching ? (
        <OperationsMessageRow message="loading detail" />
      ) : null}
      {detailQuery.isError ? (
        <OperationsMessageRow
          message={errorMessage(detailQuery.error)}
          tone="error"
        />
      ) : null}
      <OperationsKeyValueRows
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
      {detailQuery.isFetching ? (
        <OperationsMessageRow message="loading detail" />
      ) : null}
      {detailQuery.isError ? (
        <OperationsMessageRow
          message={errorMessage(detailQuery.error)}
          tone="error"
        />
      ) : null}
      <OperationsKeyValueRows
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

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : "Runtime request failed";
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
