import { Activity, AlertTriangle, Clock, Inbox } from "lucide-react";

import { StatusPill } from "../components/runtime/status-pill";
import {
  useRuntimeSummary,
  type RuntimeSummary,
} from "../hooks/use-runtime-queries";
import { relativeAge, time } from "../lib/format";

export function OverviewPage() {
  const summaryQuery = useRuntimeSummary();
  const summary = summaryQuery.data;
  const activity = summary?.recentActivity.slice(0, 8) ?? [];
  const failures = summary?.recentFailures.slice(0, 6) ?? [];

  return (
    <section className="runtime-grid grid h-full min-h-0 grid-rows-[auto_minmax(0,1fr)] overflow-hidden bg-[var(--background)] text-[var(--foreground)]">
      <header className="soft-panel border-b px-3 py-2">
        <div className="flex items-center gap-2">
          <Activity className="text-[var(--accent)]" size={14} />
          <h1 className="font-mono text-[13px] font-semibold">
            Runtime Overview
          </h1>
          <span className="ml-auto font-mono text-[10px] text-[var(--muted)]">
            status{" "}
            {summaryQuery.isError ? "degraded" : (summary?.status ?? "loading")}{" "}
            / mock
          </span>
        </div>
      </header>

      <div className="grid min-h-0 grid-cols-[minmax(0,1.35fr)_360px] overflow-hidden max-xl:grid-cols-1">
        <main className="grid min-h-0 grid-rows-[auto_minmax(0,1fr)] overflow-hidden border-r border-[var(--border-subtle)] bg-[color-mix(in_srgb,var(--surface)_72%,var(--background))] max-xl:border-r-0">
          <SummaryStrip summary={summary} />
          <div className="min-h-0 overflow-auto">
            <SectionHeader
              title="Recent Activity"
              meta="ordered by created_at"
            />
            {summaryQuery.isLoading ? (
              <LoadingRows />
            ) : summaryQuery.isError ? (
              <MessageRow
                message={errorMessage(summaryQuery.error)}
                tone="error"
              />
            ) : activity.length === 0 ? (
              <MessageRow message="No recent runtime activity" />
            ) : (
              activity.map((item) => (
                <div
                  className="grid min-h-11 grid-cols-[108px_minmax(0,1fr)_96px_120px] items-center gap-3 border-b border-[var(--border-subtle)] px-3 font-mono text-[11px] transition hover:bg-[var(--hover)]"
                  key={item.id}
                >
                  <StatusPill status={item.status} />
                  <div className="min-w-0">
                    <div className="truncate text-[var(--foreground)]">
                      {item.name}
                    </div>
                    <div className="truncate text-[10px] text-[var(--muted)]">
                      {item.id}
                    </div>
                  </div>
                  <span className="text-[var(--muted)]">
                    {item.attempts}/{item.maxAttempts}
                  </span>
                  <span className="truncate text-right text-[var(--muted)]">
                    {time(item.createdAt)}
                  </span>
                </div>
              ))
            )}
          </div>
        </main>

        <aside className="grid min-h-0 grid-rows-[auto_minmax(0,1fr)] overflow-hidden bg-[color-mix(in_srgb,var(--surface)_94%,var(--background))] shadow-[inset_1px_0_0_color-mix(in_srgb,var(--border)_55%,transparent)] max-xl:hidden">
          <SectionHeader title="Failure Stream" meta="operator attention" />
          <div className="min-h-0 overflow-auto">
            {failures.length === 0 ? (
              <MessageRow message="No failed or dead runtime work" />
            ) : (
              failures.map((failure) => (
                <div
                  className="border-b border-[var(--border-subtle)] px-3 py-2 font-mono text-[11px] transition hover:bg-[var(--hover)]"
                  key={failure.id}
                >
                  <div className="mb-1 flex items-center gap-2">
                    <StatusPill status={failure.status} />
                    <span className="ml-auto text-[10px] text-[var(--muted)]">
                      {relativeAge(failure.createdAt)}
                    </span>
                  </div>
                  <div className="truncate text-[var(--foreground)]">
                    {failure.name}
                  </div>
                  <div className="truncate text-[10px] text-[var(--muted)]">
                    {failure.correlationId}
                  </div>
                  {failure.lastError ? (
                    <div className="mt-1 text-[10px] leading-4 text-[var(--error)]">
                      {failure.lastError}
                    </div>
                  ) : null}
                </div>
              ))
            )}
          </div>
        </aside>
      </div>
    </section>
  );
}

function SummaryStrip({ summary }: { summary: RuntimeSummary | undefined }) {
  const rows = summary
    ? [
        {
          icon: <Inbox size={13} />,
          label: "outbox pending",
          value: summary.outbox.pending,
          note: ageNote(summary.outbox.oldestPendingAgeSeconds),
        },
        {
          icon: <Clock size={13} />,
          label: "functions running",
          value: summary.functions.running,
          note: ageNote(summary.functions.oldestPendingAgeSeconds),
        },
        {
          icon: <AlertTriangle size={13} />,
          label: "function failures",
          value: summary.functions.failed,
          note: failedNote(summary.functions.oldestFailedAgeSeconds),
        },
        {
          icon: <AlertTriangle size={13} />,
          label: "dead letters",
          value: summary.outbox.dead + summary.functions.dead,
          note: "operator action",
        },
      ]
    : [];

  return (
    <div className="grid border-b border-[var(--border-subtle)] bg-[color-mix(in_srgb,var(--surface)_92%,transparent)] md:grid-cols-4">
      {rows.length === 0 ? (
        <MessageRow message="Runtime summary unavailable" />
      ) : (
        rows.map((row) => (
          <div
            className="grid grid-cols-[16px_minmax(0,1fr)_auto] items-center gap-2 border-r border-[var(--border-subtle)] px-3 py-2 font-mono text-[10px] last:border-r-0"
            key={row.label}
          >
            <span className="text-[var(--muted)]">{row.icon}</span>
            <span className="min-w-0 truncate text-[var(--secondary)]">
              {row.label}
            </span>
            <span className="text-[13px] font-semibold text-[var(--foreground)]">
              {row.value}
            </span>
            <span className="col-span-3 truncate text-[var(--muted)]">
              {row.note}
            </span>
          </div>
        ))
      )}
    </div>
  );
}

function SectionHeader({ title, meta }: { title: string; meta: string }) {
  return (
    <div className="flex h-8 items-center gap-2 border-b border-[var(--border-subtle)] bg-[color-mix(in_srgb,var(--surface)_92%,transparent)] px-3">
      <span className="font-mono text-[10px] font-semibold uppercase tracking-[0.08em] text-[var(--secondary)]">
        {title}
      </span>
      <span className="ml-auto font-mono text-[10px] text-[var(--muted)]">
        {meta}
      </span>
    </div>
  );
}

function LoadingRows() {
  return (
    <>
      <div className="h-11 animate-pulse border-b border-[var(--border-subtle)] bg-[var(--elevated)]" />
      <div className="h-11 animate-pulse border-b border-[var(--border-subtle)] bg-[var(--elevated)]" />
      <div className="h-11 animate-pulse border-b border-[var(--border-subtle)] bg-[var(--elevated)]" />
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
      className={`border-b border-[var(--border-subtle)] bg-[color-mix(in_srgb,var(--surface)_42%,transparent)] px-3 py-3 font-mono text-[11px] ${
        tone === "error" ? "text-[var(--error)]" : "text-[var(--muted)]"
      }`}
    >
      {message}
    </div>
  );
}

function ageNote(seconds?: number) {
  return seconds === undefined
    ? "no pending work"
    : `oldest pending ${seconds}s`;
}

function failedNote(seconds?: number) {
  return seconds === undefined ? "retryable" : `oldest failed ${seconds}s`;
}

function errorMessage(error: unknown) {
  return error instanceof Error
    ? error.message
    : "Runtime summary request failed";
}
