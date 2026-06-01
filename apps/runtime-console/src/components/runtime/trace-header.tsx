import { AlertCircle, ChevronRight, Clock, Layers, X } from "lucide-react";
import { Fragment } from "react";

import type { TraceRun, TraceSpan } from "../../data/mock-runtime";
import {
  criticalPath,
  formatTraceDuration,
  serviceColor,
  traceStats,
} from "../../lib/trace-style";

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
    <header className="border-b border-[var(--border-subtle)] bg-[var(--surface)]">
      <div className="flex items-center gap-2 px-3 pt-2 pb-1">
        <div
          className="flex-shrink-0 rounded-[2px] border px-1.5 py-0.5 font-mono text-[10px] font-medium tracking-wide"
          style={{
            backgroundColor: `${serviceColor(trace.service)}14`,
            borderColor: `${serviceColor(trace.service)}28`,
            color: serviceColor(trace.service),
          }}
        >
          {root?.service ?? trace.service}
        </div>
        <h1 className="min-w-0 flex-1 truncate font-mono text-[13px] font-semibold leading-tight text-[var(--foreground)]">
          {root?.name ?? trace.name}
        </h1>
        <button
          aria-label="Close trace detail"
          className="grid size-5 place-items-center rounded-[2px] text-[var(--muted)] transition hover:bg-[var(--hover)] hover:text-[var(--foreground)]"
          type="button"
        >
          <X size={13} />
        </button>
      </div>

      <div className="flex flex-wrap items-center gap-1.5 px-3 pb-1.5">
        <span className="font-mono text-[10px] text-[var(--muted)]">
          {trace.id.slice(0, 16)}
        </span>
        <div className="h-3 w-px bg-[var(--border-subtle)]" />
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
        <div className="flex h-1 overflow-hidden border border-[var(--border-subtle)] bg-[var(--elevated)]">
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
        <div className="flex items-center gap-1 overflow-x-auto px-3 pb-1.5">
          {path.map((span, index) => (
            <Fragment key={span.id}>
              {index > 0 ? (
                <ChevronRight className="size-2.5 flex-shrink-0 text-[var(--muted)]" />
              ) : null}
              <button
                className="max-w-[140px] flex-shrink-0 truncate rounded-[2px] px-1 py-0.5 font-mono text-[10px] text-[var(--secondary)] transition hover:bg-[var(--hover)] hover:text-[var(--foreground)]"
                onClick={() => onSelectSpan(span)}
                title={span.name}
                type="button"
              >
                {span.name}
              </button>
            </Fragment>
          ))}
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
    accent: "text-[var(--accent)]",
    error: "text-[#ef4444]",
    muted: "text-[var(--secondary)]",
  }[tone];

  return (
    <span
      className={`inline-flex items-center gap-1 rounded-[2px] border border-[var(--border-subtle)] bg-[var(--elevated)] px-1.5 py-0.5 font-mono text-[10px] leading-none ${toneClass}`}
    >
      {icon}
      {children}
    </span>
  );
}
