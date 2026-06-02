import { Activity, AlertTriangle, Clock, Inbox } from "lucide-react";

import { RuntimeStatusBadge } from "../components/runtime/runtime-status-badge";
import {
  type RuntimeHeatmap,
  useRuntimeSummary,
  useRuntimeHeatmap,
  type RuntimeSummary,
} from "../hooks/use-runtime-queries";
import { relativeAge, time } from "../lib/format";
import { runtimeConsoleDataSource } from "../lib/http-client";

export function OverviewPage() {
  const summaryQuery = useRuntimeSummary();
  const heatmapQuery = useRuntimeHeatmap();
  const summary = summaryQuery.data;
  const heatmap = heatmapQuery.data;
  const activity = summary?.recentActivity.slice(0, 8) ?? [];
  const failures = summary?.recentFailures.slice(0, 6) ?? [];

  return (
    <section className="runtime-grid grid h-full min-h-0 grid-rows-[auto_minmax(0,1fr)] overflow-hidden bg-(--background) text-(--foreground)">
      <header className="soft-panel border-b px-3 py-2">
        <div className="flex items-center gap-2">
          <Activity className="text-(--accent)" size={14} />
          <h1 className="font-mono text-[13px] font-semibold">
            Runtime Overview
          </h1>
          <span className="ml-auto font-mono text-[10px] text-(--muted)">
            status{" "}
            {summaryQuery.isError ? "degraded" : (summary?.status ?? "loading")}{" "}
            / {runtimeConsoleDataSource()}
          </span>
        </div>
      </header>

      <div className="grid min-h-0 grid-cols-[minmax(0,1.35fr)_360px] overflow-hidden max-xl:grid-cols-1">
        <main className="grid min-h-0 grid-rows-[auto_auto_minmax(0,1fr)] overflow-hidden border-r border-(--border-subtle) bg-[color-mix(in_srgb,var(--surface)_72%,var(--background))] max-xl:border-r-0">
          <SummaryStrip summary={summary} />
          <RuntimeHeatmapStrip
            heatmap={heatmap}
            loading={heatmapQuery.isLoading}
          />
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
                  className="grid min-h-11 grid-cols-[108px_minmax(0,1fr)_96px_120px] items-center gap-3 border-b border-(--border-subtle) px-3 font-mono text-[11px] transition hover:bg-(--hover)"
                  key={item.id}
                >
                  <RuntimeStatusBadge
                    className="w-[72px] justify-center"
                    status={item.status}
                    variant="label"
                  />
                  <div className="min-w-0">
                    <div className="truncate text-(--foreground)">
                      {item.name}
                    </div>
                    <div className="truncate text-[10px] text-(--muted)">
                      {item.id}
                    </div>
                  </div>
                  <span className="text-(--muted)">
                    {item.attempts}/{item.maxAttempts}
                  </span>
                  <span className="truncate text-right text-(--muted)">
                    {time(item.createdAt)}
                  </span>
                </div>
              ))
            )}
          </div>
        </main>

        <aside className="grid min-h-0 grid-rows-[auto_minmax(0,1fr)] overflow-hidden bg-[color-mix(in_srgb,var(--surface)_94%,var(--background))] max-xl:hidden">
          <SectionHeader title="Failure Stream" meta="operator attention" />
          <div className="min-h-0 overflow-auto">
            {failures.length === 0 ? (
              <MessageRow message="No failed or dead runtime work" />
            ) : (
              failures.map((failure) => (
                <div
                  className="border-b border-(--border-subtle) px-3 py-2 font-mono text-[11px] transition hover:bg-(--hover)"
                  key={failure.id}
                >
                  <div className="mb-1 flex items-center gap-2">
                    <RuntimeStatusBadge
                      className="w-[72px] justify-center"
                      status={failure.status}
                      variant="label"
                    />
                    <span className="ml-auto text-[10px] text-(--muted)">
                      {relativeAge(failure.createdAt)}
                    </span>
                  </div>
                  <div className="truncate text-(--foreground)">
                    {failure.name}
                  </div>
                  <div className="truncate text-[10px] text-(--muted)">
                    {failure.correlationId}
                  </div>
                  {failure.lastError ? (
                    <div className="mt-1 text-[10px] leading-4 text-(--error)">
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

function RuntimeHeatmapStrip({
  heatmap,
  loading,
}: {
  heatmap: RuntimeHeatmap | undefined;
  loading: boolean;
}) {
  const cells = heatmap?.cells.slice(0, 18) ?? [];

  return (
    <div className="border-b border-(--border-subtle) bg-[color-mix(in_srgb,var(--surface)_86%,transparent)] px-3 py-2">
      <div className="mb-2 flex items-center gap-2">
        <span className="font-mono text-[10px] font-semibold uppercase tracking-[0.08em] text-(--secondary)">
          Runtime Heatmap
        </span>
        <span className="ml-auto font-mono text-[10px] text-(--muted)">
          {heatmap ? `${heatmap.bucketSeconds}s buckets` : "loading"}
        </span>
      </div>
      <div className="grid grid-cols-[repeat(18,minmax(0,1fr))] gap-1">
        {loading && cells.length === 0
          ? Array.from({ length: 18 }, (_, index) => (
              <span
                className="h-7 animate-pulse border border-(--border-subtle) bg-(--elevated)"
                key={index}
              />
            ))
          : cells.map((cell, index) => (
              <span
                className="h-7 border border-(--border-subtle)"
                key={`${cell.bucketStart}-${cell.service}-${cell.nodeType}-${index}`}
                style={{
                  backgroundColor: heatmapColor(cell),
                }}
                title={`${cell.service} ${cell.nodeType}: ${cell.totalCount} total, ${cell.errorCount} errors`}
              />
            ))}
        {!loading && cells.length === 0 ? (
          <span
            className="font-mono text-[11px] text-(--muted)"
            style={{ gridColumn: "1 / -1" }}
          >
            No runtime heatmap data
          </span>
        ) : null}
      </div>
    </div>
  );
}

function heatmapColor(cell: RuntimeHeatmap["cells"][number]) {
  if (cell.deadCount > 0) {
    return "rgba(239,68,68,0.82)";
  }
  if (cell.errorCount > 0) {
    return "rgba(251,191,36,0.72)";
  }
  if ((cell.maxDurationMs ?? 0) > 30_000) {
    return "color-mix(in srgb, var(--accent) 70%, transparent)";
  }
  if (cell.totalCount > 1) {
    return "rgba(34,197,94,0.48)";
  }
  return "rgba(59,130,246,0.32)";
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
    <div className="grid border-b border-(--border-subtle) bg-[color-mix(in_srgb,var(--surface)_92%,transparent)] md:grid-cols-4">
      {rows.length === 0 ? (
        <MessageRow message="Runtime summary unavailable" />
      ) : (
        rows.map((row) => (
          <div
            className="grid grid-cols-[16px_minmax(0,1fr)_auto] items-center gap-2 border-r border-(--border-subtle) px-3 py-2 font-mono text-[10px] last:border-r-0"
            key={row.label}
          >
            <span className="text-(--muted)">{row.icon}</span>
            <span className="min-w-0 truncate text-(--secondary)">
              {row.label}
            </span>
            <span className="text-[13px] font-semibold text-(--foreground)">
              {row.value}
            </span>
            <span className="col-span-3 truncate text-(--muted)">
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
    <div className="flex h-8 items-center gap-2 border-b border-(--border-subtle) bg-[color-mix(in_srgb,var(--surface)_92%,transparent)] px-3">
      <span className="font-mono text-[10px] font-semibold uppercase tracking-[0.08em] text-(--secondary)">
        {title}
      </span>
      <span className="ml-auto font-mono text-[10px] text-(--muted)">
        {meta}
      </span>
    </div>
  );
}

function LoadingRows() {
  return (
    <>
      <div className="h-11 animate-pulse border-b border-(--border-subtle) bg-(--elevated)" />
      <div className="h-11 animate-pulse border-b border-(--border-subtle) bg-(--elevated)" />
      <div className="h-11 animate-pulse border-b border-(--border-subtle) bg-(--elevated)" />
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
      className={`border-b border-(--border-subtle) bg-[color-mix(in_srgb,var(--surface)_42%,transparent)] px-3 py-3 font-mono text-[11px] ${
        tone === "error" ? "text-(--error)" : "text-(--muted)"
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
