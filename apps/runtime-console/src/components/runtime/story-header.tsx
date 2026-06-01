import { AlertCircle, Boxes, Clock, Server, X } from "lucide-react";

import type { RuntimeStory, ExecutionNode } from "../../data/mock-runtime";
import { cn } from "../../lib/cn";
import { formatRuntimeDuration } from "../../lib/runtime-style";
import { buildRuntimeStory } from "../../lib/story";
import { HorizontalScrollArea } from "./horizontal-tab-scroll";
import { RuntimeStatusBadge } from "./runtime-status-badge";

export function StoryHeader({
  onClose,
  onSelectNode,
  story,
}: {
  onClose: () => void;
  story: RuntimeStory;
  onSelectNode: (node: ExecutionNode) => void;
}) {
  const storySummary = buildRuntimeStory(story);
  const isError =
    storySummary.status === "failed" || storySummary.status === "dead";

  return (
    <header className="min-w-0 overflow-hidden border-b border-(--border-subtle) bg-(--surface)">
      <div className="flex min-w-0 items-start gap-3 px-3 pt-2 pb-1.5">
        <div className="min-w-0 flex-1">
          <div className="flex min-w-0 items-center gap-2">
            <h1 className="min-w-0 truncate text-[16px] font-semibold leading-tight text-(--foreground)">
              {storySummary.title}
            </h1>
            <RuntimeStatusBadge status={storySummary.status} />
          </div>
          <div className="mt-1 flex min-w-0 flex-wrap items-center gap-1.5 font-mono text-[10px] text-(--secondary)">
            <Metric icon={<Clock size={10} />} tone="accent">
              {formatRuntimeDuration(storySummary.duration)}
            </Metric>
            <Metric icon={<Boxes size={10} />}>
              {storySummary.nodeCount} nodes
            </Metric>
            <Metric
              icon={<AlertCircle size={10} />}
              tone={storySummary.errorCount > 0 ? "error" : "muted"}
            >
              {storySummary.errorCount} errors
            </Metric>
            <Metric icon={<Server size={10} />}>
              {storySummary.services.length} services
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
            {storySummary.services.map((service) => (
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
          {storySummary.patternLabel || "No execution pattern"}
        </span>
        <span className="text-(--muted-deep)">·</span>
        <span className="min-w-0 truncate text-(--muted)">
          {storySummary.correlationId}
        </span>
        {storySummary.rootError ? (
          <>
            <span className="text-(--muted-deep)">·</span>
            <button
              className={cn(
                "min-w-0 truncate text-left text-[#ff8b86] transition hover:text-[#ffd0cd]",
                isError && "font-semibold"
              )}
              onClick={() => {
                const errorNode = lastErrorNode(storySummary.nodes);
                if (errorNode) {
                  onSelectNode(errorNode.node);
                }
              }}
              type="button"
            >
              {storySummary.rootError}
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
