import { useNavigate } from "@tanstack/react-router";
import { ExternalLink, Inbox, RefreshCcw } from "lucide-react";
import { useMemo, useState } from "react";

import { ResizeHandle } from "../components/runtime/resize-handle";
import { Button } from "../components/ui/button";
import { buildRuntimeQueueRows } from "../hooks/runtime-queue-model";
import { useListKeyboard } from "../hooks/use-list-keyboard";
import {
  useRuntimeEvents,
  useRuntimeFunctions,
  useRuntimeSummary,
} from "../hooks/use-runtime-queries";
import { runtimeConsoleDataSource } from "../lib/http-client";
import {
  OperationsFilterBar,
  OperationsSearchInput,
} from "./operations-filter";
import { useOperationsInspectorLayout } from "./operations-layout";
import { useOperationsSelection } from "./operations-selection";
import { OperationsMessageRow } from "./operations-state";
import {
  OperationsKeyValueRows,
  OperationsSelectableRow,
  OperationsTableHeader,
} from "./operations-table";
import { queuesPath } from "./operations-url-model";
import {
  readOperationsParamValue,
  useOperationsUrlPopState,
} from "./operations-url-state";
import {
  buildQueueRowsFromSummary,
  filterQueueRows,
  queueRouteTarget,
  queueRowId,
  type QueueRow,
} from "./queues-model";

export function QueuesPage() {
  const navigate = useNavigate();
  const [query, setQuery] = useState(() => readOperationsParamValue("q"));
  const [selectedId, setSelectedId] = useState(() =>
    readOperationsParamValue("selected")
  );
  const { inspectorWidth, resetLayout, resizeInspector } =
    useOperationsInspectorLayout({
      defaultWidth: 376,
      maxWidth: 560,
      minWidth: 320,
      storageKey: "runtime-console:queues-layout",
    });
  const summaryQuery = useRuntimeSummary();
  const eventsQuery = useRuntimeEvents();
  const functionsQuery = useRuntimeFunctions();
  const allRows = useMemo(
    () =>
      eventsQuery.data && functionsQuery.data
        ? buildRuntimeQueueRows({
            events: eventsQuery.data,
            functions: functionsQuery.data,
          })
        : buildQueueRowsFromSummary(summaryQuery.data),
    [eventsQuery.data, functionsQuery.data, summaryQuery.data]
  );
  const rows = useMemo(() => filterQueueRows(allRows, query), [allRows, query]);
  const totals = allRows.reduce(
    (acc, queue) => ({
      dead: acc.dead + queue.dead,
      failed: acc.failed + queue.failed,
      pending: acc.pending + queue.pending,
      running: acc.running + queue.running,
    }),
    { dead: 0, failed: 0, pending: 0, running: 0 }
  );
  const isRefetching =
    summaryQuery.isRefetching ||
    eventsQuery.isRefetching ||
    functionsQuery.isRefetching;

  useOperationsUrlPopState([
    { name: "q", setValue: setQuery },
    { name: "selected", setValue: setSelectedId },
  ]);

  const refreshQueues = () => {
    void Promise.all([
      summaryQuery.refetch(),
      eventsQuery.refetch(),
      functionsQuery.refetch(),
    ]);
  };
  const { selected, selectedIndex, selectIndex, selectItem } =
    useOperationsSelection({
      currentPath: queuesPath({ query, selectedId }),
      getId: queueRowId,
      items: rows,
      pathForSelectedId: (nextSelectedId) =>
        queuesPath({ query, selectedId: nextSelectedId }),
      selectedId,
      setSelectedId,
    });
  const selectedTarget = selected ? queueRouteTarget(selected) : null;
  useListKeyboard({
    items: rows,
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
      <main className="grid min-h-0 min-w-0 grid-rows-[auto_auto_auto_minmax(0,1fr)] overflow-hidden border-r border-(--border-subtle)">
        <header className="border-b border-(--border-subtle) bg-(--surface) px-3 py-2">
          <div className="flex items-center gap-2">
            <Inbox className="text-(--accent)" size={14} />
            <h1 className="font-mono text-[13px] font-semibold">Queues</h1>
            <span className="ml-auto font-mono text-[10px] text-(--muted)">
              {rows.length} queues / {runtimeConsoleDataSource()}
            </span>
          </div>
        </header>

        <div className="grid border-b border-(--border-subtle) bg-(--surface) md:grid-cols-4">
          {[
            ["pending", totals.pending],
            ["running", totals.running],
            ["failed", totals.failed],
            ["dead", totals.dead],
          ].map(([label, value]) => (
            <div
              className="grid grid-cols-[minmax(0,1fr)_auto] border-r border-(--border-subtle) px-3 py-2 font-mono text-[10px] last:border-r-0"
              key={label}
            >
              <span className="text-(--muted)">{label}</span>
              <span className="text-[13px] font-semibold text-(--foreground)">
                {value}
              </span>
            </div>
          ))}
        </div>

        <OperationsFilterBar>
          <OperationsSearchInput
            ariaLabel="Search queues"
            onChange={setQuery}
            placeholder="queue / count / age"
            value={query}
          />
        </OperationsFilterBar>

        <div className="min-h-0 overflow-auto">
          <OperationsTableHeader className="grid-cols-[minmax(180px,1fr)_72px_72px_72px_72px_92px_minmax(120px,240px)] gap-2">
            <span>queue</span>
            <span>pending</span>
            <span>running</span>
            <span>failed</span>
            <span>dead</span>
            <span>oldest</span>
            <span>pressure</span>
          </OperationsTableHeader>
          {summaryQuery.isLoading ||
          eventsQuery.isLoading ||
          functionsQuery.isLoading ? (
            <OperationsMessageRow message="loading queue pressure" />
          ) : summaryQuery.isError && rows.length === 0 ? (
            <OperationsMessageRow
              message={
                summaryQuery.error instanceof Error
                  ? summaryQuery.error.message
                  : "Queue pressure unavailable"
              }
              tone="error"
            />
          ) : rows.length === 0 ? (
            <OperationsMessageRow message="no queues matched" />
          ) : (
            rows.map((queue) => {
              const total =
                queue.pending + queue.running + queue.failed + queue.dead;
              const isSelected = selected?.name === queue.name;
              return (
                <OperationsSelectableRow
                  className="min-h-11 grid-cols-[minmax(180px,1fr)_72px_72px_72px_72px_92px_minmax(120px,240px)] gap-2"
                  isSelected={isSelected}
                  key={queue.name}
                  onClick={() => selectItem(queue)}
                >
                  <span className="truncate text-(--foreground)">
                    {queue.name}
                  </span>
                  <span className="text-(--secondary)">{queue.pending}</span>
                  <span className="text-(--secondary)">{queue.running}</span>
                  <span
                    className={
                      queue.failed > 0 ? "text-[#ef4444]" : "text-(--muted)"
                    }
                  >
                    {queue.failed}
                  </span>
                  <span
                    className={
                      queue.dead > 0 ? "text-[#ef4444]" : "text-(--muted)"
                    }
                  >
                    {queue.dead}
                  </span>
                  <span className="text-(--muted)">
                    {formatOldest(queue.oldestSeconds)}
                  </span>
                  <span className="flex min-w-0 items-center gap-2">
                    <span className="h-1 flex-1 overflow-hidden rounded-[1px] bg-(--elevated)">
                      <span
                        className="block h-full rounded-[1px] bg-(--accent)"
                        style={{
                          width: `${Math.min(100, Math.max(4, total * 5))}%`,
                        }}
                      />
                    </span>
                    <span className="w-8 text-right text-(--muted)">
                      {total}
                    </span>
                  </span>
                </OperationsSelectableRow>
              );
            })
          )}
        </div>
      </main>

      <ResizeHandle
        ariaLabel="Resize queue inspector panel"
        onReset={resetLayout}
        onResize={resizeInspector}
      />

      <aside className="grid min-h-0 min-w-0 grid-rows-[auto_minmax(0,1fr)_auto] overflow-hidden bg-(--sidebar)">
        <header className="border-b border-(--border-subtle) bg-(--surface) px-3 py-2 font-mono">
          <div className="mb-1 text-[9px] font-semibold uppercase tracking-[0.12em] text-(--accent)">
            Queue
          </div>
          <div className="truncate text-[13px] font-semibold text-(--foreground)">
            {selected?.name ?? "No queue selected"}
          </div>
          {selected ? (
            <div className="mt-1 text-[10px] text-(--muted)">
              {selectedTarget?.reason}
            </div>
          ) : null}
        </header>
        <div className="min-h-0 overflow-auto">
          {selected ? (
            <QueueInspector queue={selected} />
          ) : (
            <OperationsMessageRow message="select a queue" />
          )}
        </div>
        <div className="flex gap-2 border-t border-(--border-subtle) bg-(--surface) p-2">
          <Button
            disabled={!selectedTarget}
            onClick={() => {
              if (selectedTarget) {
                navigate({ to: selectedTarget.path });
              }
            }}
            variant="ghost"
          >
            <ExternalLink size={13} />
            {selectedTarget?.label ?? "Open"}
          </Button>
          <Button
            disabled={isRefetching}
            onClick={refreshQueues}
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

function QueueInspector({ queue }: { queue: QueueRow }) {
  const total = queue.pending + queue.running + queue.failed + queue.dead;
  return (
    <OperationsKeyValueRows
      rowClassName="grid-cols-[108px_minmax(180px,max-content)]"
      valueClassName=""
      rows={[
        ["pending", String(queue.pending)],
        ["running", String(queue.running)],
        ["failed", String(queue.failed)],
        ["dead", String(queue.dead)],
        ["oldest", formatOldest(queue.oldestSeconds)],
        ["pressure", String(total)],
      ]}
    />
  );
}

function formatOldest(seconds: number | undefined) {
  if (seconds === undefined) {
    return "-";
  }
  if (seconds < 60) {
    return `${seconds}s`;
  }
  return `${Math.round(seconds / 60)}m`;
}
