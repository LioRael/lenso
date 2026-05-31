import { AlertTriangle, Clock, Cpu, Inbox, Radio } from "lucide-react";
import type { ReactNode } from "react";

import { StatusPill } from "../components/runtime/status-pill";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import { Card } from "../components/ui/card";
import { Panel } from "../components/ui/panel";
import {
  useRuntimeSummary,
  type RuntimeSummary,
} from "../hooks/use-runtime-queries";
import { relativeAge, time } from "../lib/format";

export function OverviewPage() {
  const summaryQuery = useRuntimeSummary();
  const summary = summaryQuery.data;
  const pendingEvents = summary?.outbox.pending ?? 0;
  const runningFunctions = summary?.functions.running ?? 0;
  const failedFunctions = summary?.functions.failed ?? 0;
  const deadLetters =
    (summary?.outbox.dead ?? 0) + (summary?.functions.dead ?? 0);
  const activity = summary?.recentActivity.slice(0, 6) ?? [];
  const failures = summary?.recentFailures ?? [];

  return (
    <section>
      <div className="mb-5 flex items-end justify-between gap-6 max-sm:block">
        <div>
          <h1 className="text-2xl font-semibold text-slate-100">
            Runtime Overview
          </h1>
          <p className="mt-1.5 max-w-2xl text-[13px] leading-6 text-slate-400">
            Live operational posture for events, function runs, retries, and
            dead letters.
          </p>
        </div>
        <Badge className="max-sm:mt-3">
          <Radio size={13} />
          {summaryQuery.isError
            ? "degraded"
            : (summary?.status ?? "loading")} ·{" "}
          mock
        </Badge>
      </div>

      <div className="grid grid-cols-4 gap-3.5 max-lg:grid-cols-2 max-sm:grid-cols-1">
        <MetricCard
          icon={<Inbox size={15} />}
          label="Pending Events"
          value={pendingEvents}
          note={ageNote(summary?.outbox.oldestPendingAgeSeconds)}
        />
        <MetricCard
          icon={<Cpu size={15} />}
          label="Running Functions"
          value={runningFunctions}
          note={ageNote(summary?.functions.oldestPendingAgeSeconds)}
        />
        <MetricCard
          icon={<AlertTriangle size={15} />}
          label="Failed Functions"
          value={failedFunctions}
          note={failedNote(summary?.functions.oldestFailedAgeSeconds)}
        />
        <MetricCard
          icon={<Clock size={15} />}
          label="Dead Letters"
          value={deadLetters}
          note="operator action needed"
        />
      </div>

      <div className="mt-3.5 grid grid-cols-[minmax(0,1.5fr)_minmax(320px,0.9fr)] gap-3.5 max-lg:grid-cols-1">
        <div className="grid gap-3.5">
          <Panel>
            <Panel.Header>
              <Panel.Title>Recent Activity</Panel.Title>
              <span className="text-xs text-slate-500">
                ordered by created_at
              </span>
            </Panel.Header>
            <Panel.Content className="grid">
              {summaryQuery.isLoading ? (
                <LoadingRows />
              ) : summaryQuery.isError ? (
                <ErrorState message={errorMessage(summaryQuery.error)} />
              ) : activity.length === 0 ? (
                <EmptyRow label="No recent runtime activity" />
              ) : (
                activity.map((item) => (
                  <div
                    className="grid grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-2.5 border-b border-white/10 px-3.5 py-3 last:border-b-0"
                    key={item.id}
                  >
                    <StatusPill status={item.status} />
                    <div className="min-w-0">
                      <div className="truncate text-[13px] font-semibold text-slate-100">
                        {item.name}
                      </div>
                      <div className="mono mt-0.5 truncate text-xs text-slate-500">
                        {item.correlationId}
                      </div>
                    </div>
                    <span className="text-xs text-slate-500">
                      {time(item.createdAt)}
                    </span>
                  </div>
                ))
              )}
            </Panel.Content>
          </Panel>

          <Panel>
            <Panel.Header>
              <Panel.Title>Queue Health</Panel.Title>
              <span className="text-xs text-slate-500">runtime stores</span>
            </Panel.Header>
            <Panel.Content className="grid">
              {summary ? (
                runtimeQueues(summary).map((queue) => (
                  <div
                    className="border-b border-white/10 px-3.5 py-3 last:border-b-0"
                    key={queue.name}
                  >
                    <div className="min-w-0">
                      <div className="mono text-[13px] font-semibold text-slate-100">
                        {queue.name}
                      </div>
                      <div className="mt-0.5 text-xs text-slate-500">
                        {queue.oldest}
                      </div>
                    </div>
                    <div className="mt-3 flex flex-wrap gap-x-3.5 gap-y-2 text-xs text-slate-500">
                      <span>pending {queue.pending}</span>
                      <span>running {queue.running}</span>
                      <span>failed {queue.failed}</span>
                      <span>dead {queue.dead}</span>
                    </div>
                  </div>
                ))
              ) : (
                <div
                  className="px-3.5 py-3 text-xs text-slate-500"
                  key="queue-loading"
                >
                  Runtime summary unavailable.
                </div>
              )}
            </Panel.Content>
          </Panel>
        </div>

        <Panel>
          <Panel.Header>
            <Panel.Title>Recent Failures</Panel.Title>
            <span className="text-xs text-slate-500">needs attention</span>
          </Panel.Header>
          <Panel.Content className="grid">
            {summaryQuery.isLoading ? (
              <LoadingRows />
            ) : failures.length === 0 ? (
              <EmptyRow label="No failed or dead runtime work" />
            ) : (
              failures.map((failure) => (
                <div
                  className="border-b border-white/10 px-3.5 py-3.5 last:border-b-0"
                  key={failure.id}
                >
                  <div className="flex flex-wrap items-center gap-x-3.5 gap-y-2">
                    <StatusPill status={failure.status} />
                    <span className="text-xs text-slate-500">
                      {relativeAge(failure.createdAt)}
                    </span>
                  </div>
                  <div className="mt-3 truncate text-[13px] font-semibold text-slate-100">
                    {failure.name}
                  </div>
                  <div className="mono mt-0.5 truncate text-xs text-slate-500">
                    {failure.correlationId}
                  </div>
                  <div className="mono mt-2 text-xs text-rose-100">
                    {failure.lastError ?? "unknown error"}
                  </div>
                  <div className="mt-3 flex gap-2.5">
                    <Button variant="ghost">Timeline</Button>
                    <Button variant="danger">Retry</Button>
                  </div>
                </div>
              ))
            )}
          </Panel.Content>
        </Panel>
      </div>
    </section>
  );
}

function runtimeQueues(summary: RuntimeSummary) {
  return [
    {
      name: "outbox",
      pending: summary.outbox.pending,
      running: summary.outbox.processing,
      failed: summary.outbox.failed,
      dead: summary.outbox.dead,
      oldest: ageNote(summary.outbox.oldestPendingAgeSeconds),
    },
    {
      name: "runtime.functions",
      pending: summary.functions.pending,
      running: summary.functions.running,
      failed: summary.functions.failed,
      dead: summary.functions.dead,
      oldest: ageNote(summary.functions.oldestPendingAgeSeconds),
    },
  ];
}

function ageNote(seconds?: number) {
  return seconds === undefined
    ? "no pending work"
    : `oldest pending ${seconds}s`;
}

function failedNote(seconds?: number) {
  return seconds === undefined ? "retryable" : `oldest failed ${seconds}s`;
}

function LoadingRows() {
  return (
    <>
      <div className="h-14 animate-pulse border-b border-white/10 bg-white/[0.03]" />
      <div className="h-14 animate-pulse border-b border-white/10 bg-white/[0.03]" />
      <div className="h-14 animate-pulse bg-white/[0.03]" />
    </>
  );
}

function ErrorState({ message }: { message: string }) {
  return (
    <div className="m-3 rounded-lg border border-rose-300/30 bg-black/20 p-3 text-xs text-rose-100">
      {message}
    </div>
  );
}

function EmptyRow({ label }: { label: string }) {
  return (
    <div className="px-3.5 py-8 text-center text-xs text-slate-500">
      {label}
    </div>
  );
}

function errorMessage(error: unknown) {
  return error instanceof Error
    ? error.message
    : "Runtime summary request failed";
}

function MetricCard({
  icon,
  label,
  value,
  note,
}: {
  icon: ReactNode;
  label: string;
  value: number;
  note: string;
}) {
  return (
    <Card>
      <Card.Content>
        <header className="flex items-center justify-between text-xs text-slate-400">
          <span>{label}</span>
          {icon}
        </header>
        <div className="mt-4 text-3xl font-bold text-slate-100">{value}</div>
        <div className="mt-1 text-xs text-slate-500">{note}</div>
      </Card.Content>
    </Card>
  );
}
