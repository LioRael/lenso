import { Search } from "lucide-react";

import type { TraceRun } from "../../data/mock-runtime";
import { cn } from "../../lib/cn";
import { time } from "../../lib/format";
import {
  formatTraceDuration,
  statusColor,
  traceStats,
} from "../../lib/trace-style";

export function TraceList({
  query,
  selectedTraceId,
  setQuery,
  traces,
  onSelect,
}: {
  traces: TraceRun[];
  selectedTraceId: string | null;
  query: string;
  setQuery: (query: string) => void;
  onSelect: (trace: TraceRun) => void;
}) {
  return (
    <aside className="grid h-full min-h-0 min-w-0 grid-rows-[auto_auto_auto_minmax(0,1fr)] overflow-hidden bg-[var(--background)]">
      <div className="flex min-h-10 items-center justify-between gap-2 border-b border-[var(--border-subtle)] bg-[var(--surface)] px-3 py-2">
        <div>
          <h2 className="font-mono text-sm font-semibold tracking-tight text-[var(--foreground)]">
            Traces
          </h2>
          <p className="font-mono text-xs text-[var(--muted)]">
            {traces.length} recorded sessions
          </p>
        </div>
      </div>
      <div className="flex h-8 items-center gap-2 border-b border-[var(--border-subtle)] px-3 text-[var(--muted)]">
        <Search size={12} />
        <input
          aria-label="Search traces"
          className="mono w-full bg-transparent text-xs text-[var(--foreground)] outline-none placeholder:text-[var(--muted)]"
          onChange={(event) => setQuery(event.target.value)}
          placeholder="filter traces / service / correlation..."
          value={query}
        />
      </div>
      <div className="grid h-7 grid-cols-[12px_minmax(0,1fr)_64px] items-center gap-2 border-b border-[var(--border-subtle)] px-3 font-mono text-[11px] font-semibold uppercase tracking-[0.04em] text-[var(--muted)]">
        <span />
        <span>trace</span>
        <span className="text-right">duration</span>
      </div>
      <div className="min-h-0 overflow-auto">
        {traces.map((trace) => {
          const stats = traceStats(trace);
          return (
            <button
              className={cn(
                "w-full border-b border-[var(--border-subtle)] px-3 py-2 text-left transition",
                selectedTraceId === trace.id
                  ? "bg-[var(--accent-soft)] shadow-[inset_2px_0_0_var(--accent)]"
                  : "hover:bg-[var(--elevated)]"
              )}
              key={trace.id}
              onClick={() => onSelect(trace)}
            >
              <div className="mb-1 flex items-center gap-1.5">
                <span
                  className="size-1.5 rounded-full"
                  style={{
                    backgroundColor: statusColor(trace.status),
                    boxShadow:
                      trace.status === "failed" || trace.status === "dead"
                        ? `0 0 8px ${statusColor(trace.status)}`
                        : undefined,
                  }}
                />
                <span className="min-w-0 flex-1 truncate font-mono text-[13px] font-medium text-[var(--foreground)]">
                  {trace.name}
                </span>
                <span className="font-mono text-[11px] text-[var(--muted)]">
                  {formatTraceDuration(trace.durationMs)}
                </span>
              </div>
              <div className="grid grid-cols-[82px_minmax(0,1fr)_auto] items-center gap-2 font-mono text-[11px] leading-5 text-[var(--muted)]">
                <code className="text-xs text-[var(--secondary)]">
                  {trace.id.slice(0, 10)}
                </code>
                <span className="truncate">
                  {trace.service}/{trace.source} · {stats.services.join(", ")}
                </span>
                <span>{time(trace.timestamp)}</span>
              </div>
              {stats.errors > 0 ? (
                <div className="mt-1 font-mono text-[11px] leading-4 text-[#ef4444]">
                  {stats.errors} error spans
                </div>
              ) : null}
            </button>
          );
        })}
      </div>
    </aside>
  );
}
