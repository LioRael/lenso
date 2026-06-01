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
    <aside className="grid h-full min-h-0 min-w-0 grid-rows-[auto_auto_auto_minmax(0,1fr)] overflow-hidden bg-(--background)">
      <div className="flex min-h-10 items-center justify-between gap-2 border-b border-(--border-subtle) bg-(--surface) px-3 py-2">
        <div>
          <h2 className="font-mono text-sm font-semibold tracking-tight text-(--foreground)">
            Traces
          </h2>
          <p className="font-mono text-xs text-(--muted)">
            {traces.length} recorded sessions
          </p>
        </div>
      </div>
      <div className="flex h-8 items-center gap-2 border-b border-(--border-subtle) px-3 text-(--muted)">
        <Search size={12} />
        <input
          aria-label="Search traces"
          className="mono w-full bg-transparent text-xs text-(--foreground) outline-hidden placeholder:text-(--muted)"
          onChange={(event) => setQuery(event.target.value)}
          placeholder="filter traces / service / correlation..."
          value={query}
        />
      </div>
      <div className="grid h-6 grid-cols-[12px_minmax(0,1fr)_58px] items-center gap-2 border-b border-(--border-subtle) px-3 font-mono text-[10px] font-semibold uppercase tracking-[0.06em] text-(--muted)">
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
                "w-full border-b border-(--border-subtle) px-3 py-2 text-left transition",
                selectedTraceId === trace.id
                  ? "bg-(--accent-soft) shadow-[inset_2px_0_0_var(--accent)]"
                  : "hover:bg-(--elevated)"
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
                <span className="min-w-0 flex-1 truncate font-mono text-xs font-medium text-(--foreground)">
                  {trace.name}
                </span>
                <span className="font-mono text-[10px] text-(--muted)">
                  {formatTraceDuration(trace.durationMs)}
                </span>
              </div>
              <div className="grid grid-cols-[74px_minmax(0,1fr)_auto] items-center gap-2 font-mono text-[10px] leading-5 text-(--muted)">
                <code className="text-[11px] text-(--secondary)">
                  {trace.id.slice(0, 10)}
                </code>
                <span className="truncate">
                  {trace.service}/{trace.source} · {stats.services.join(", ")}
                </span>
                <span>{time(trace.timestamp)}</span>
              </div>
              {stats.errors > 0 ? (
                <div className="mt-1 font-mono text-[10px] leading-4 text-[#ef4444]">
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
