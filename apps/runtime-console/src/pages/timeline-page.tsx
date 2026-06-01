import { ArrowRight, GitBranch, Search } from "lucide-react";
import { useMemo, useState } from "react";

import {
  resolveTimelineSource,
  useRuntimeConsole,
} from "../components/runtime/runtime-console-context";
import { StatusPill } from "../components/runtime/status-pill";
import {
  TimelineNode,
  TimelineNodeIcon,
} from "../components/runtime/timeline-node";
import { Button } from "../components/ui/button";
import { EmptyState } from "../components/ui/empty-state";
import { Panel } from "../components/ui/panel";
import {
  correlationId,
  futureTimelineSlots,
  retryTargetFor,
  type RuntimeStatus,
} from "../data/mock-runtime";
import { useRuntimeTimeline } from "../hooks/use-runtime-queries";
import { duration } from "../lib/format";
import { runtimeConsoleDataSource } from "../lib/http-client";

export function TimelinePage() {
  const { activeCorrelationId, openDrawer, openRetry, openTimeline } =
    useRuntimeConsole();
  const [input, setInput] = useState(activeCorrelationId || correlationId);
  const timelineQuery = useRuntimeTimeline(activeCorrelationId);
  const items = useMemo(() => timelineQuery.data ?? [], [timelineQuery.data]);

  const summary = useMemo(() => {
    const failed = items.filter((item) => item.status === "failed").length;
    const dead = items.filter((item) => item.status === "dead").length;
    const completed = items.filter(
      (item) => item.status === "completed" || item.status === "published"
    ).length;
    const totalDuration =
      items.length > 0
        ? duration(
            items[0]?.startedAt ?? items[0]?.createdAt,
            items.at(-1)?.completedAt
          )
        : "-";
    return {
      completed,
      dead,
      failed,
      status: (dead > 0
        ? "dead"
        : failed > 0
          ? "failed"
          : "completed") as RuntimeStatus,
      totalDuration,
    };
  }, [items]);

  return (
    <section>
      <div className="mb-4 grid grid-cols-[minmax(0,1fr)_minmax(280px,420px)] items-end gap-4 max-lg:grid-cols-1">
        <div>
          <p className="mb-2 text-[11px] font-semibold uppercase tracking-[0.08em] text-slate-500">
            Execution causality
          </p>
          <h1 className="text-3xl font-bold text-slate-100">Timeline</h1>
          <p className="mt-2 max-w-3xl text-[13px] leading-6 text-slate-400">
            Follow one correlation from HTTP request through domain command,
            event relay, and runtime function execution.
          </p>
        </div>
        <Panel className="p-3.5">
          <StatusPill status={summary.status} />
          <div className="mt-3 grid grid-cols-2 gap-x-3.5 gap-y-2 text-xs text-slate-400">
            <span>{items.length} nodes</span>
            <span>{summary.completed} completed</span>
            <span>{summary.failed} failed</span>
            <span>{summary.dead} dead</span>
            <span>{summary.totalDuration}</span>
          </div>
        </Panel>
      </div>

      <form
        className="mb-3 flex items-center gap-2.5"
        onSubmit={(event) => {
          event.preventDefault();
          openTimeline(input.trim());
        }}
      >
        <label className="flex h-9 min-w-[min(520px,100%)] items-center gap-2.5 rounded-lg border border-white/10 bg-white/3.5 px-3 text-slate-400">
          <Search size={15} />
          <input
            aria-label="Correlation ID"
            className="mono w-full bg-transparent text-[13px] text-slate-100 outline-hidden"
            onChange={(event) => setInput(event.target.value)}
            value={input}
          />
        </label>
        <Button type="submit">
          Open Timeline
          <ArrowRight size={15} />
        </Button>
      </form>

      <Panel>
        <Panel.Header>
          <div className="min-w-0">
            <div className="mono truncate text-[13px] font-semibold text-slate-100">
              {activeCorrelationId}
            </div>
            <div className="mt-0.5 text-xs text-slate-500">
              ordered by created_at ascending · {runtimeConsoleDataSource()}{" "}
              runtime story
            </div>
          </div>
          <StatusPill status={summary.status} />
        </Panel.Header>

        {timelineQuery.isLoading ? (
          <div className="grid gap-3 p-4.5">
            <div className="h-24 animate-pulse rounded-lg bg-white/3" />
            <div className="h-24 animate-pulse rounded-lg bg-white/3" />
            <div className="h-24 animate-pulse rounded-lg bg-white/3" />
          </div>
        ) : timelineQuery.isError ? (
          <EmptyState>
            <EmptyState.Icon>
              <GitBranch size={22} />
            </EmptyState.Icon>
            <EmptyState.Title>Timeline request failed</EmptyState.Title>
            <EmptyState.Description>
              {errorMessage(timelineQuery.error)}
            </EmptyState.Description>
          </EmptyState>
        ) : items.length === 0 ? (
          <EmptyState>
            <EmptyState.Icon>
              <GitBranch size={22} />
            </EmptyState.Icon>
            <EmptyState.Title>No timeline nodes found</EmptyState.Title>
            <EmptyState.Description>
              <span className="mono">{activeCorrelationId}</span>
            </EmptyState.Description>
          </EmptyState>
        ) : (
          <div className="p-4.5">
            {items.map((item, index) => (
              <TimelineNode
                index={index}
                item={item}
                key={item.id}
                onOpen={() =>
                  openDrawer(
                    resolveTimelineSource(item.detailId ?? item.id) ?? {
                      item,
                      kind: "timeline",
                    }
                  )
                }
                onRetry={() => {
                  const record = resolveTimelineSource(
                    item.detailId ?? item.id
                  ) ?? {
                    item,
                    kind: "timeline" as const,
                  };
                  const retryTarget = retryTargetFor(record);
                  if (retryTarget) {
                    openRetry(retryTarget);
                  }
                }}
              />
            ))}

            <div className="ml-23 mt-3 grid grid-cols-3 gap-2.5 max-lg:ml-0 max-md:grid-cols-1">
              {futureTimelineSlots.map((slot) => (
                <div
                  className="grid grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-2.5 rounded-lg border border-dashed border-white/15 bg-white/[0.018] p-2.5 text-xs text-slate-500"
                  key={slot.type}
                >
                  <TimelineNodeIcon type={slot.type} />
                  <span>{slot.name}</span>
                  <small>reserved</small>
                </div>
              ))}
            </div>
          </div>
        )}
      </Panel>
    </section>
  );
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : "Runtime request failed";
}
