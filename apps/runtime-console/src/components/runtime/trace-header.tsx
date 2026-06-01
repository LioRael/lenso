import { AlertCircle, Clock, Layers, X } from "lucide-react";
import { Fragment } from "react";

import type { TraceRun, TraceSpan } from "../../data/mock-runtime";
import { buildRuntimeStory } from "../../lib/story";
import {
  formatTraceDuration,
  serviceColor,
  traceStats,
} from "../../lib/trace-style";
import { HorizontalScrollArea } from "./horizontal-tab-scroll";
import { StatusPill } from "./status-pill";

export function TraceHeader({
  onClose,
  onSelectSpan,
  trace,
}: {
  onClose: () => void;
  trace: TraceRun;
  onSelectSpan: (span: TraceSpan) => void;
}) {
  const stats = traceStats(trace);
  const story = buildRuntimeStory(trace);

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
          Story
        </div>
        <h1 className="min-w-0 flex-1 truncate text-[15px] font-semibold leading-tight text-(--foreground)">
          {story.title}
        </h1>
        <StatusPill status={story.status} />
        <button
          aria-label="Close story detail"
          className="grid size-5 place-items-center rounded-xs text-(--muted) transition hover:bg-(--hover) hover:text-(--foreground)"
          onClick={onClose}
          type="button"
        >
          <X size={13} />
        </button>
      </div>

      <div className="flex min-w-0 flex-wrap items-center gap-1.5 overflow-hidden px-3 pb-1.5">
        <span className="shrink-0 font-mono text-[10px] text-(--muted)">
          {story.correlationId}
        </span>
        <div className="h-3 w-px shrink-0 bg-(--border-subtle)" />
        <MetricChip icon={<Clock size={10} />} tone="accent">
          {formatTraceDuration(story.durationMs)}
        </MetricChip>
        <MetricChip icon={<Layers size={10} />}>
          {story.spanCount} spans
        </MetricChip>
        <MetricChip>{stats.services.length} svc</MetricChip>
        {story.errorCount > 0 ? (
          <MetricChip icon={<AlertCircle size={10} />} tone="error">
            {story.errorCount} errors
          </MetricChip>
        ) : null}
      </div>

      <div className="px-3 pb-1.5">
        <div className="flex h-1 overflow-hidden border border-(--border-subtle) bg-(--elevated)">
          {stats.services.map((service) => {
            const duration = trace.spans
              .filter((span) => span.service === service)
              .reduce((total, span) => total + span.durationMs, 0);
            const width =
              trace.durationMs > 0
                ? Math.max(2, (duration / trace.durationMs) * 100)
                : 100;
            return (
              <div
                className="h-full"
                key={service}
                style={{
                  backgroundColor: serviceColor(service),
                  opacity: 0.8,
                  width: `${width}%`,
                }}
              />
            );
          })}
        </div>
      </div>

      {story.nodes.length > 1 ? (
        <div className="min-w-0 px-3 pb-1.5">
          <HorizontalScrollArea
            className="h-5"
            contentClassName="h-full"
            viewportClassName="h-full"
          >
            <div className="flex h-full w-max min-w-full items-center gap-1">
              {story.nodes.map((node, index) => (
                <Fragment key={node.id}>
                  {index > 0 ? (
                    <span className="shrink-0 text-[10px] text-(--muted)">
                      {">"}
                    </span>
                  ) : null}
                  <button
                    className="max-w-42 shrink-0 truncate rounded-xs px-1 py-0.5 font-mono text-[10px] text-(--secondary) transition hover:bg-(--hover) hover:text-(--foreground)"
                    onClick={() => onSelectSpan(node.span)}
                    title={node.title}
                    type="button"
                  >
                    {node.title}
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
