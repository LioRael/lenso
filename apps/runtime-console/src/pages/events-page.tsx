import { RotateCcw, Search } from "lucide-react";
import { useEffect, useMemo, useState } from "react";

import { useRuntimeConsole } from "../components/runtime/runtime-console-context";
import { StatusPill } from "../components/runtime/status-pill";
import { Button } from "../components/ui/button";
import { EmptyState as EmptyStateView } from "../components/ui/empty-state";
import { Panel } from "../components/ui/panel";
import {
  retryTargetFor,
  type RuntimeEvent,
  type RuntimeStatus,
} from "../data/mock-runtime";
import { useListKeyboard } from "../hooks/use-list-keyboard";
import { useRuntimeEvents } from "../hooks/use-runtime-queries";
import { time } from "../lib/format";

const statuses: Array<RuntimeStatus | "all"> = [
  "all",
  "pending",
  "processing",
  "published",
  "failed",
  "dead",
];

export function EventsPage() {
  const { openDrawer, openRetry } = useRuntimeConsole();
  const [query, setQuery] = useState("");
  const [status, setStatus] = useState<RuntimeStatus | "all">("all");
  const [selectedIndex, setSelectedIndex] = useState(0);
  const eventsQuery = useRuntimeEvents();

  const filtered = useMemo(() => {
    const normalized = query.trim().toLowerCase();
    return (eventsQuery.data ?? []).filter((event) => {
      const matchesQuery =
        normalized.length === 0 ||
        event.eventName.toLowerCase().includes(normalized) ||
        event.correlationId.toLowerCase().includes(normalized) ||
        event.id.toLowerCase().includes(normalized);
      const matchesStatus = status === "all" || event.status === status;
      return matchesQuery && matchesStatus;
    });
  }, [eventsQuery.data, query, status]);

  useEffect(() => {
    setSelectedIndex(0);
  }, [query, status]);

  const selected = filtered[selectedIndex] ?? null;
  const openEvent = (event: RuntimeEvent) =>
    openDrawer({ kind: "event", item: event });
  const retryEvent = (event: RuntimeEvent) => {
    const retryTarget = retryTargetFor({ kind: "event", item: event });
    if (retryTarget) {
      openRetry(retryTarget);
    }
  };

  useListKeyboard({
    items: filtered,
    selectedIndex,
    setSelectedIndex,
    onOpen: openEvent,
    onRetry: retryEvent,
  });

  return (
    <section>
      <div className="mb-5 flex items-end justify-between gap-6">
        <div>
          <h1 className="text-2xl font-semibold text-(--foreground)">Events</h1>
          <p className="mt-1.5 max-w-2xl text-[13px] leading-6 text-(--secondary)">
            Explore outbox events by status, event name, and correlation ID.
          </p>
        </div>
      </div>

      <div className="mb-3 flex flex-wrap items-center gap-2.5">
        <label className="flex h-9 min-w-[min(420px,100%)] items-center gap-2.5 rounded-lg border border-(--border-subtle) bg-(--elevated) px-3 text-(--secondary)">
          <Search size={15} />
          <input
            aria-label="Search events"
            className="w-full bg-transparent text-[13px] text-(--foreground) outline-hidden placeholder:text-(--muted)"
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Search event, id, correlation..."
            value={query}
          />
        </label>
        <select
          aria-label="Filter event status"
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
        <div className="grid grid-cols-[110px_minmax(230px,1fr)_78px_132px_154px] border-b border-(--border-subtle) px-3.5 py-2.5 text-[11px] font-semibold uppercase tracking-[0.04em] text-(--muted) max-md:hidden">
          <span>status</span>
          <span>event</span>
          <span>attempts</span>
          <span>created</span>
          <span>correlation</span>
        </div>
        <div className="grid">
          {eventsQuery.isLoading ? (
            <LoadingRows />
          ) : eventsQuery.isError ? (
            <ErrorState message={errorMessage(eventsQuery.error)} />
          ) : filtered.length === 0 ? (
            <EmptyState label="No events matched this view" />
          ) : (
            filtered.map((event) => (
              <button
                className={`grid w-full grid-cols-[110px_minmax(230px,1fr)_78px_132px_154px] items-center gap-2.5 border-b border-(--border-subtle) bg-transparent px-3.5 py-3 text-left text-(--foreground) last:border-b-0 hover:bg-(--hover) max-md:grid-cols-1 ${
                  selected?.id === event.id ? "bg-(--hover)" : ""
                }`}
                key={event.id}
                onClick={() => {
                  setSelectedIndex(indexOf(filtered, event.id));
                  openEvent(event);
                }}
              >
                <StatusPill status={event.status} />
                <div className="min-w-0">
                  <div className="truncate text-[13px] font-semibold text-(--foreground)">
                    {event.eventName}
                  </div>
                  <div className="mono mt-0.5 truncate text-xs text-(--muted)">
                    {event.id}
                  </div>
                </div>
                <span className="mono">
                  {event.attempts}/{event.maxAttempts}
                </span>
                <span>{time(event.createdAt)}</span>
                <span className="mono truncate text-xs text-(--muted)">
                  {event.correlationId}
                </span>
              </button>
            ))
          )}
        </div>
      </Panel>

      <div className="mt-3 flex justify-end">
        <Button
          disabled={!selected}
          onClick={() => selected && retryEvent(selected)}
          variant="danger"
        >
          <RotateCcw size={15} />
          Retry selected
        </Button>
      </div>
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

function indexOf(items: RuntimeEvent[], id: string) {
  return Math.max(
    0,
    items.findIndex((item) => item.id === id)
  );
}
