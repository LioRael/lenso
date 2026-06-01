import { AlertCircle, Boxes, Clock, Server, X } from "lucide-react";

import type { TraceRun, TraceSpan } from "../../data/mock-runtime";
import { cn } from "../../lib/cn";
import { buildRuntimeStory } from "../../lib/story";
import { formatTraceDuration } from "../../lib/trace-style";
import { HorizontalScrollArea } from "./horizontal-tab-scroll";
import { TraceStatusBadge } from "./trace-status-badge";

export function TraceHeader({
  onClose,
  onSelectSpan,
  trace,
}: {
  onClose: () => void;
  trace: TraceRun;
  onSelectSpan: (span: TraceSpan) => void;
}) {
  const story = buildRuntimeStory(trace);
  const isError = story.status === "failed" || story.status === "dead";

  return (
    <header className="min-w-0 overflow-hidden border-b border-(--border-subtle) bg-(--surface)">
      <div className="flex min-w-0 items-start gap-3 px-3 pt-2 pb-1.5">
        <div className="min-w-0 flex-1">
          <div className="flex min-w-0 items-center gap-2">
            <h1 className="min-w-0 truncate text-[16px] font-semibold leading-tight text-(--foreground)">
              {story.title}
            </h1>
            <TraceStatusBadge status={story.status} />
          </div>
          <div className="mt-1 flex min-w-0 flex-wrap items-center gap-1.5 font-mono text-[10px] text-(--secondary)">
            <Metric icon={<Clock size={10} />} tone="accent">
              {formatTraceDuration(story.duration)}
            </Metric>
            <Metric icon={<Boxes size={10} />}>{story.nodeCount} nodes</Metric>
            <Metric
              icon={<AlertCircle size={10} />}
              tone={story.errorCount > 0 ? "error" : "muted"}
            >
              {story.errorCount} errors
            </Metric>
            <Metric icon={<Server size={10} />}>
              {story.services.length} services
            </Metric>
          </div>
        </div>

        <button
          aria-label="Close story detail"
          className="grid size-5 shrink-0 place-items-center rounded-xs text-(--muted) transition hover:bg-(--hover) hover:text-(--foreground)"
          onClick={onClose}
          type="button"
        >
          <X size={13} />
        </button>
      </div>

      <div className="min-w-0 px-3 pb-1.5">
        <HorizontalScrollArea className="h-6" viewportClassName="h-full">
          <div className="flex h-full w-max min-w-full items-center gap-1.5">
            {story.services.map((service) => (
              <span
                className="shrink-0 border border-(--border-subtle) bg-(--elevated) px-1.5 py-0.5 font-mono text-[10px] text-(--secondary)"
                key={service}
              >
                {service}
              </span>
            ))}
          </div>
        </HorizontalScrollArea>
      </div>

      <div className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1 px-3 pb-1.5 font-mono text-[10px]">
        <span className="min-w-0 truncate text-(--secondary)">
          {story.patternLabel || "No execution pattern"}
        </span>
        <span className="text-(--muted-deep)">·</span>
        <span className="min-w-0 truncate text-(--muted)">
          {story.correlationId}
        </span>
        {story.rootError ? (
          <>
            <span className="text-(--muted-deep)">·</span>
            <button
              className={cn(
                "min-w-0 truncate text-left text-[#ff8b86] transition hover:text-[#ffd0cd]",
                isError && "font-semibold"
              )}
              onClick={() => {
                const errorNode = lastErrorNode(story.nodes);
                if (errorNode) {
                  onSelectSpan(errorNode.span);
                }
              }}
              type="button"
            >
              {story.rootError}
            </button>
          </>
        ) : null}
      </div>
    </header>
  );
}

function lastErrorNode(nodes: ReturnType<typeof buildRuntimeStory>["nodes"]) {
  for (let index = nodes.length - 1; index >= 0; index -= 1) {
    const node = nodes[index];
    if (node?.error) {
      return node;
    }
  }

  return null;
}

function Metric({
  children,
  icon,
  tone = "muted",
}: {
  children: React.ReactNode;
  icon: React.ReactNode;
  tone?: "accent" | "error" | "muted";
}) {
  const toneClass = {
    accent: "text-(--accent)",
    error: "text-[#ff8b86]",
    muted: "text-(--secondary)",
  }[tone];

  return (
    <span className={cn("inline-flex items-center gap-1", toneClass)}>
      {icon}
      {children}
    </span>
  );
}
