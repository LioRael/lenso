import { Search } from "lucide-react";

import type { TraceRun } from "../../data/mock-runtime";
import { cn } from "../../lib/cn";
import { time } from "../../lib/format";

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
    <aside className="min-h-0 border-r border-white/10 bg-[#07080a]">
      <div className="flex h-8 items-center gap-2 border-b border-white/10 px-2 text-slate-600">
        <Search size={13} />
        <input
          aria-label="Search traces"
          className="mono w-full bg-transparent text-[11px] text-slate-200 outline-none placeholder:text-slate-700"
          onChange={(event) => setQuery(event.target.value)}
          placeholder="filter traces / service / correlation..."
          value={query}
        />
      </div>
      <div className="grid h-6 grid-cols-[14px_minmax(0,1fr)_52px] items-center gap-2 border-b border-white/[0.07] px-2 font-mono text-[9px] uppercase tracking-[0.04em] text-slate-700">
        <span />
        <span>trace</span>
        <span className="text-right">duration</span>
      </div>
      <div className="h-[calc(100%-56px)] overflow-auto">
        {traces.map((trace) => (
          <button
            className={cn(
              "grid w-full grid-cols-[12px_minmax(0,1fr)_54px] gap-2 border-b border-white/[0.065] bg-transparent px-2 py-1.5 text-left font-mono text-[11px] text-slate-500 hover:bg-white/[0.035]",
              selectedTraceId === trace.id &&
                "bg-cyan-300/[0.055] text-slate-100"
            )}
            key={trace.id}
            onClick={() => onSelect(trace)}
          >
            <span
              className={cn(
                "mt-1.5 size-1.5 rounded-full",
                statusDot(trace.status)
              )}
            />
            <span className="min-w-0">
              <span className="block truncate text-slate-200">
                {trace.name}
              </span>
              <span className="mt-0.5 grid grid-cols-[minmax(0,1fr)_auto] gap-2 text-slate-600">
                <span className="truncate text-slate-600">
                  {trace.service}/{trace.source}
                </span>
                <span className="text-slate-700">{time(trace.timestamp)}</span>
              </span>
            </span>
            <span className="text-right text-slate-500">
              {formatMs(trace.durationMs)}
            </span>
          </button>
        ))}
      </div>
    </aside>
  );
}

function statusDot(status: TraceRun["status"]) {
  if (status === "failed" || status === "dead") {
    return "bg-rose-400 shadow-[0_0_14px_rgba(251,113,133,0.8)]";
  }
  if (status === "pending" || status === "processing" || status === "running") {
    return "bg-amber-300";
  }
  return "bg-emerald-300";
}

function formatMs(value: number) {
  return value >= 1000 ? `${(value / 1000).toFixed(2)}s` : `${value}ms`;
}
