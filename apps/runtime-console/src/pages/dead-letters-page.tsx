import { RotateCcw, Search } from "lucide-react";
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
import { useListKeyboard } from "../hooks/use-list-keyboard";
import { usePersistedLayout } from "../hooks/use-persisted-layout";
import { useDeadLetters } from "../hooks/use-runtime-queries";
import { time } from "../lib/format";

type DeadLetter =
  | { kind: "event"; item: RuntimeEvent }
  | { kind: "function"; item: FunctionRun };

const deadLettersLayoutDefaults = {
  inspectorWidth: 376,
};

function clamp(value: number, min: number, max: number) {
  return Math.min(max, Math.max(min, value));
}

export function DeadLettersPage() {
  const { openRetry, openTimeline } = useRuntimeConsole();
  const [query, setQuery] = useState("");
  const [kind, setKind] = useState<"all" | "event" | "function">("all");
  const [oldestFirst, setOldestFirst] = useState(true);
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
  useEffect(() => setSelectedIndex(0), [kind, oldestFirst, query]);

  const selected = visible[selectedIndex] ?? null;
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
      inspectorWidth: clamp(
        (current.inspectorWidth ?? deadLettersLayoutDefaults.inspectorWidth) -
          deltaX,
        320,
        560
      ),
    }));
  };

  useListKeyboard({
    items: visible,
    selectedIndex,
    setSelectedIndex,
    onOpen: (failure) => setSelectedIndex(indexOf(visible, failure.item.id)),
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
            <h1 className="font-mono text-[13px] font-semibold">
              Dead Letters
            </h1>
            <span className="ml-auto font-mono text-[10px] text-(--muted)">
              {visible.length} failures / mock
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
              onClick={() => setKind(item)}
              type="button"
            >
              {item}
            </button>
          ))}
          <button
            className="h-6 border border-(--border-subtle) px-2 font-mono text-[10px] text-(--muted) hover:text-(--foreground)"
            onClick={() => setOldestFirst((current) => !current)}
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
                  onClick={() => setSelectedIndex(indexOf(visible, item.id))}
                  type="button"
                >
                  <StatusPill status={item.status} />
                  <span className="min-w-0">
                    <span className="block truncate text-(--foreground)">
                      {name}
                    </span>
                    <span className="block truncate text-[10px] text-(--muted)">
                      {failure.kind} / {item.id} / {item.correlationId}
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
              selected && openTimeline(selected.item.correlationId)
            }
            variant="ghost"
          >
            Traces
          </Button>
          <Button
            disabled={!selected}
            onClick={() => selected && retryFailure(selected)}
            variant="danger"
          >
            <RotateCcw size={13} />
            Retry
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
  return (
    <header className="border-b border-(--border-subtle) bg-(--surface) px-3 py-2 font-mono">
      <div className="mb-1 text-[9px] font-semibold uppercase tracking-[0.12em] text-(--accent)">
        {failure?.kind ?? "Failure"}
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
  const { item } = failure;
  const detail =
    failure.kind === "event" ? failure.item.payload : failure.item.input;
  return (
    <div className="grid gap-3 p-3">
      <KeyValueRows
        rows={[
          ["status", item.status],
          ["attempts", `${item.attempts}/${item.maxAttempts}`],
          ["correlation", item.correlationId],
          ["created", item.createdAt],
          ["last_error", item.lastError ?? "-"],
        ]}
      />
      <JsonViewer
        defaultExpanded
        title={failure.kind === "event" ? "event payload" : "function input"}
        value={detail}
      />
    </div>
  );
}

function KeyValueRows({ rows }: { rows: Array<[string, string]> }) {
  return (
    <div className="border-y border-(--border-subtle) font-mono text-[11px]">
      {rows.map(([key, value]) => (
        <div
          className="grid grid-cols-[94px_minmax(0,1fr)] border-b border-(--border-subtle) last:border-b-0"
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
