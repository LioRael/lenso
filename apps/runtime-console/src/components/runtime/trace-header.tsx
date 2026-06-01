import { AlertCircle, ChevronRight, Clock, Layers, X } from "lucide-react";
import { Fragment } from "react";

import type { TraceRun, TraceSpan } from "../../data/mock-runtime";
import {
  criticalPath,
  formatTraceDuration,
  serviceColor,
  traceStats,
} from "../../lib/trace-style";
import { HorizontalScrollArea } from "./horizontal-tab-scroll";

export function TraceHeader({
  trace,
  onSelectSpan,
}: {
  trace: TraceRun;
  onSelectSpan: (span: TraceSpan) => void;
}) {
  const stats = traceStats(trace);
  const path = criticalPath(trace);
  const [root] = trace.spans;

  return (
    <header className="min-w-0 overflow-hidden border-b border-(--border-subtle) bg-(--surface)">
      <div className="flex min-w-0 items-center gap-2 px-3 pt-2 pb-1">
        <div
          className="shrink-0 rounded-xs border px-1.5 py-0.5 font-mono text-[10px] font-medium tracking-wide"
          style={{
            backgroundColor: `${serviceColor(trace.service)}14`,
            borderColor: `${serviceColor(trace.service)}28`,
            color: serviceColor(trace.service),
          }}
        >
          {root?.service ?? trace.service}
        </div>
        <h1 className="min-w-0 flex-1 truncate font-mono text-[13px] font-semibold leading-tight text-(--foreground)">
          {root?.name ?? trace.name}
        </h1>
        <button
          aria-label="Close trace detail"
          className="grid size-5 place-items-center rounded-xs text-(--muted) transition hover:bg-(--hover) hover:text-(--foreground)"
          type="button"
        >
          <X size={13} />
        </button>
      </div>

      <div className="flex min-w-0 flex-wrap items-center gap-1.5 overflow-hidden px-3 pb-1.5">
        <span className="shrink-0 font-mono text-[10px] text-(--muted)">
          {trace.id.slice(0, 16)}
        </span>
        <div className="h-3 w-px shrink-0 bg-(--border-subtle)" />
        <MetricChip icon={<Clock size={10} />} tone="accent">
          {formatTraceDuration(trace.durationMs)}
        </MetricChip>
        <MetricChip icon={<Layers size={10} />}>
          {stats.spanCount} spans
        </MetricChip>
        <MetricChip>{stats.services.length} svc</MetricChip>
        {stats.errors > 0 ? (
          <MetricChip icon={<AlertCircle size={10} />} tone="error">
            {stats.errors} err
          </MetricChip>
        ) : null}
      </div>

      <div className="px-3 pb-1.5">
        <div className="flex h-1 overflow-hidden border border-(--border-subtle) bg-(--elevated)">
          {stats.services.map((service) => {
            const duration = trace.spans
              .filter((span) => span.service === service)
              .reduce((total, span) => total + span.durationMs, 0);
            return (
              <div
                className="h-full"
                key={service}
                style={{
                  backgroundColor: serviceColor(service),
                  opacity: 0.8,
                  width: `${Math.max(2, (duration / trace.durationMs) * 100)}%`,
                }}
              />
            );
          })}
        </div>
      </div>

      {path.length > 1 ? (
        <div className="min-w-0 px-3 pb-1.5">
          <HorizontalScrollArea
            className="h-5"
            contentClassName="h-full"
            viewportClassName="h-full"
          >
            <div className="flex h-full w-max min-w-full items-center gap-1">
              {path.map((span, index) => (
                <Fragment key={span.id}>
                  {index > 0 ? (
                    <ChevronRight className="size-2.5 shrink-0 text-(--muted)" />
                  ) : null}
                  <button
                    className="max-w-35 shrink-0 truncate rounded-xs px-1 py-0.5 font-mono text-[10px] text-(--secondary) transition hover:bg-(--hover) hover:text-(--foreground)"
                    onClick={() => onSelectSpan(span)}
                    title={span.name}
                    type="button"
                  >
                    {span.name}
                  </button>
                </Fragment>
              ))}
            </div>
          </HorizontalScrollArea>
        </div>
      ) : null}
    </header>
  );
}

function MetricChip({
  children,
  icon,
  tone = "muted",
}: {
  children: React.ReactNode;
  icon?: React.ReactNode;
  tone?: "accent" | "error" | "muted";
}) {
  const toneClass = {
    accent: "text-(--accent)",
    error: "text-[#ef4444]",
    muted: "text-(--secondary)",
  }[tone];

  return (
    <span
      className={`inline-flex items-center gap-1 rounded-xs border border-(--border-subtle) bg-(--elevated) px-1.5 py-0.5 font-mono text-[10px] leading-none ${toneClass}`}
    >
      {icon}
      {children}
    </span>
  );
}
