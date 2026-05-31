import { AlertTriangle, Clock, Cpu, Inbox, Radio } from "lucide-react";
import type { ReactNode } from "react";

import { StatusPill } from "../components/runtime/status-pill";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import { Card } from "../components/ui/card";
import { Panel } from "../components/ui/panel";
import { functionRuns, queueHealth, runtimeEvents } from "../data/mock-runtime";
import {
  useRuntimeEvents,
  useRuntimeFunctions,
} from "../hooks/use-runtime-queries";
import { relativeAge, time } from "../lib/format";

export function OverviewPage() {
  const eventsQuery = useRuntimeEvents();
  const functionsQuery = useRuntimeFunctions();
  const events = eventsQuery.data ?? runtimeEvents;
  const functions = functionsQuery.data ?? functionRuns;
  const pendingEvents = events.filter(
    (event) => event.status === "pending"
  ).length;
  const runningFunctions = functions.filter(
    (run) => run.status === "running"
  ).length;
  const failedFunctions = functions.filter(
    (run) => run.status === "failed"
  ).length;
  const deadLetters =
    events.filter((event) => event.status === "dead").length +
    functions.filter((run) => run.status === "dead").length;

  const activity = [
    ...events.map((event) => ({
      id: event.id,
      name: event.eventName,
      subtitle: event.correlationId,
      status: event.status,
      at: event.createdAt,
    })),
    ...functions.map((run) => ({
      id: run.id,
      name: run.functionName,
      subtitle: run.correlationId,
      status: run.status,
      at: run.createdAt,
    })),
  ]
    .sort((a, b) => b.at.localeCompare(a.at))
    .slice(0, 6);

  const failures = [
    ...events
      .filter((event) => event.status === "failed" || event.status === "dead")
      .map((event) => ({
        id: event.id,
        name: event.eventName,
        status: event.status,
        error: event.lastError ?? "unknown error",
        correlationId: event.correlationId,
      })),
    ...functions
      .filter((run) => run.status === "failed" || run.status === "dead")
      .map((run) => ({
        id: run.id,
        name: run.functionName,
        status: run.status,
        error: run.lastError ?? "unknown error",
        correlationId: run.correlationId,
      })),
  ];

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
          {eventsQuery.isError || functionsQuery.isError
            ? "degraded"
            : "healthy"}{" "}
          · {eventsQuery.data || functionsQuery.data ? "query/mock" : "mock"}
        </Badge>
      </div>

      <div className="grid grid-cols-4 gap-3.5 max-lg:grid-cols-2 max-sm:grid-cols-1">
        <MetricCard
          icon={<Inbox size={15} />}
          label="Pending Events"
          value={pendingEvents}
          note="oldest pending 38s"
        />
        <MetricCard
          icon={<Cpu size={15} />}
          label="Running Functions"
          value={runningFunctions}
          note="2 workers active"
        />
        <MetricCard
          icon={<AlertTriangle size={15} />}
          label="Failed Functions"
          value={failedFunctions}
          note="retryable"
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
              {activity.map((item) => (
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
                      {item.subtitle}
                    </div>
                  </div>
                  <span className="text-xs text-slate-500">
                    {time(item.at)}
                  </span>
                </div>
              ))}
            </Panel.Content>
          </Panel>

          <Panel>
            <Panel.Header>
              <Panel.Title>Queue Health</Panel.Title>
              <span className="text-xs text-slate-500">runtime stores</span>
            </Panel.Header>
            <Panel.Content className="grid">
              {queueHealth.map((queue) => (
                <div
                  className="border-b border-white/10 px-3.5 py-3 last:border-b-0"
                  key={queue.name}
                >
                  <div className="min-w-0">
                    <div className="mono text-[13px] font-semibold text-slate-100">
                      {queue.name}
                    </div>
                    <div className="mt-0.5 text-xs text-slate-500">
                      oldest pending {queue.oldest}
                    </div>
                  </div>
                  <div className="mt-3 flex flex-wrap gap-x-3.5 gap-y-2 text-xs text-slate-500">
                    <span>pending {queue.pending}</span>
                    <span>running {queue.running}</span>
                    <span>failed {queue.failed}</span>
                    <span>dead {queue.dead}</span>
                  </div>
                </div>
              ))}
            </Panel.Content>
          </Panel>
        </div>

        <Panel>
          <Panel.Header>
            <Panel.Title>Recent Failures</Panel.Title>
            <span className="text-xs text-slate-500">needs attention</span>
          </Panel.Header>
          <Panel.Content className="grid">
            {failures.map((failure) => (
              <div
                className="border-b border-white/10 px-3.5 py-3.5 last:border-b-0"
                key={failure.id}
              >
                <div className="flex flex-wrap items-center gap-x-3.5 gap-y-2">
                  <StatusPill status={failure.status} />
                  <span className="text-xs text-slate-500">
                    {relativeAge("2026-05-31T09:18:18.180Z")}
                  </span>
                </div>
                <div className="mt-3 truncate text-[13px] font-semibold text-slate-100">
                  {failure.name}
                </div>
                <div className="mono mt-0.5 truncate text-xs text-slate-500">
                  {failure.correlationId}
                </div>
                <div className="mono mt-2 text-xs text-rose-100">
                  {failure.error}
                </div>
                <div className="mt-3 flex gap-2.5">
                  <Button variant="ghost">Timeline</Button>
                  <Button variant="danger">Retry</Button>
                </div>
              </div>
            ))}
          </Panel.Content>
        </Panel>
      </div>
    </section>
  );
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
