import { Inbox } from "lucide-react";

import { buildRuntimeQueueRows } from "../hooks/runtime-queue-model";
import {
  useRuntimeEvents,
  useRuntimeFunctions,
  useRuntimeSummary,
} from "../hooks/use-runtime-queries";
import { runtimeConsoleDataSource } from "../lib/http-client";
import { buildQueueRowsFromSummary } from "./queues-model";

export function QueuesPage() {
  const summaryQuery = useRuntimeSummary();
  const eventsQuery = useRuntimeEvents();
  const functionsQuery = useRuntimeFunctions();
  const rows =
    eventsQuery.data && functionsQuery.data
      ? buildRuntimeQueueRows({
          events: eventsQuery.data,
          functions: functionsQuery.data,
        })
      : buildQueueRowsFromSummary(summaryQuery.data);
  const totals = rows.reduce(
    (acc, queue) => ({
      dead: acc.dead + queue.dead,
      failed: acc.failed + queue.failed,
      pending: acc.pending + queue.pending,
      running: acc.running + queue.running,
    }),
    { dead: 0, failed: 0, pending: 0, running: 0 }
  );

  return (
    <section className="grid h-full min-h-0 grid-rows-[auto_auto_minmax(0,1fr)] overflow-hidden bg-(--background) text-(--foreground)">
      <header className="border-b border-(--border-subtle) bg-(--surface) px-3 py-2">
        <div className="flex items-center gap-2">
          <Inbox className="text-(--accent)" size={14} />
          <h1 className="font-mono text-[13px] font-semibold">Queues</h1>
          <span className="ml-auto font-mono text-[10px] text-(--muted)">
            aggregate pressure / {runtimeConsoleDataSource()}
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

      <div className="min-h-0 overflow-auto">
        <div className="grid h-7 grid-cols-[minmax(180px,1fr)_72px_72px_72px_72px_92px_minmax(120px,240px)] items-center gap-2 border-b border-(--border-subtle) bg-[color-mix(in_srgb,var(--elevated)_52%,transparent)] px-3 font-mono text-[9px] uppercase tracking-[0.08em] text-(--muted)">
          <span>queue</span>
          <span>pending</span>
          <span>running</span>
          <span>failed</span>
          <span>dead</span>
          <span>oldest</span>
          <span>pressure</span>
        </div>
        {summaryQuery.isLoading ||
        eventsQuery.isLoading ||
        functionsQuery.isLoading ? (
          <QueueMessage message="Loading queue pressure..." />
        ) : summaryQuery.isError && rows.length === 0 ? (
          <QueueMessage
            message={
              summaryQuery.error instanceof Error
                ? summaryQuery.error.message
                : "Queue pressure unavailable"
            }
            tone="error"
          />
        ) : (
          rows.map((queue) => {
            const total =
              queue.pending + queue.running + queue.failed + queue.dead;
            return (
              <div
                className="grid min-h-11 grid-cols-[minmax(180px,1fr)_72px_72px_72px_72px_92px_minmax(120px,240px)] items-center gap-2 border-b border-(--border-subtle) px-3 font-mono text-[11px]"
                key={queue.name}
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
                  <span className="w-8 text-right text-(--muted)">{total}</span>
                </span>
              </div>
            );
          })
        )}
      </div>
    </section>
  );
}

function QueueMessage({
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

function formatOldest(seconds: number | undefined) {
  if (seconds === undefined) {
    return "-";
  }
  if (seconds < 60) {
    return `${seconds}s`;
  }
  return `${Math.round(seconds / 60)}m`;
}
