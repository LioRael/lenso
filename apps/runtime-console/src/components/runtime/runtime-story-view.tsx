import {
  AlertTriangle,
  Check,
  Cloud,
  Mail,
  Play,
  RefreshCcw,
  Route,
  ServerCog,
  Workflow,
  XCircle,
} from "lucide-react";
import type { ComponentType } from "react";

import type { TraceRun, TraceSpan } from "../../data/mock-runtime";
import { cn } from "../../lib/cn";
import {
  buildRuntimeStory,
  runtimeStatusIntent,
  type RuntimeNode,
  type RuntimeNodeType,
} from "../../lib/story";
import { formatTraceDuration } from "../../lib/trace-style";
import { Button } from "../ui/button";
import { TraceViewHeader } from "./trace-view-header";

export function RuntimeStoryView({
  selectedSpanId,
  trace,
  onRetryNode,
  onSelectSpan,
}: {
  trace: TraceRun;
  selectedSpanId: string | null;
  onSelectSpan: (span: TraceSpan) => void;
  onRetryNode: (node: RuntimeNode) => void;
}) {
  const story = buildRuntimeStory(trace);

  return (
    <div className="grid h-full min-h-0 min-w-0 grid-rows-[auto_minmax(0,1fr)] overflow-hidden bg-(--background)">
      <TraceViewHeader
        meta={`${story.nodeCount} nodes · ${formatTraceDuration(story.duration)}`}
        summary={story.patternLabel || "No execution pattern"}
        title="Runtime Story"
      />

      <div className="min-h-0 overflow-auto px-4 py-4">
        <div className="mx-auto grid w-full max-w-4xl gap-2">
          {story.nodes.length === 0 ? (
            <div className="border border-(--border-subtle) bg-(--surface) p-4 font-mono text-xs text-(--muted)">
              No runtime story nodes were derived for this story.
            </div>
          ) : null}

          {story.nodes.map((node, index) => (
            <GraphNode
              key={node.id}
              node={node}
              onRetry={() => onRetryNode(node)}
              onSelect={() => onSelectSpan(node.span)}
              selected={selectedSpanId === node.span.id}
              showConnector={index < story.nodes.length - 1}
            />
          ))}
        </div>
      </div>
    </div>
  );
}

function GraphNode({
  node,
  selected,
  showConnector,
  onRetry,
  onSelect,
}: {
  node: RuntimeNode;
  selected: boolean;
  showConnector: boolean;
  onSelect: () => void;
  onRetry: () => void;
}) {
  const type = nodeStyle[node.type];
  const status = statusStyle[runtimeStatusIntent(node.status)];
  const Icon = type.icon;
  const StatusIcon = status.icon;
  const retryable = node.status === "failed" || node.status === "dead";

  return (
    <div className="grid min-w-0 grid-cols-[40px_minmax(0,1fr)] gap-3">
      <div className="relative flex justify-center">
        <span
          className={cn(
            "relative z-10 mt-1 grid size-9 place-items-center border bg-(--surface)",
            type.iconClass,
            selected && "ring-2 ring-(--accent)"
          )}
        >
          <Icon size={16} strokeWidth={1.8} />
          <span
            className={cn(
              "-right-1 -bottom-1 absolute grid size-4 place-items-center rounded-full border border-(--background)",
              status.badgeClass
            )}
            title={status.label}
          >
            <StatusIcon size={10} strokeWidth={2.2} />
          </span>
        </span>
        {showConnector ? (
          <span className="absolute top-11 bottom-[-0.5rem] w-px bg-[linear-gradient(180deg,var(--border)_0%,var(--border-subtle)_100%)]" />
        ) : null}
      </div>

      <div
        className={cn(
          "group relative min-w-0 border bg-(--surface) px-3 py-2.5 text-left shadow-[0_10px_26px_var(--shadow-soft)] transition hover:-translate-y-px hover:border-(--border) hover:bg-(--elevated)",
          type.cardClass,
          selected &&
            "border-(--accent) bg-(--accent-soft) shadow-[inset_2px_0_0_var(--accent),0_14px_32px_var(--shadow-soft)]",
          (node.status === "failed" || node.status === "dead") &&
            "shadow-[inset_0_0_0_1px_rgba(239,68,68,0.18),0_12px_30px_var(--shadow-soft)]"
        )}
      >
        <button
          aria-label={`Select ${node.typeLabel} ${node.name}`}
          className="absolute inset-0 z-0 cursor-pointer focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-cyan-300"
          onClick={onSelect}
          type="button"
        />
        <span className="flex min-w-0 items-start gap-3">
          <span className="min-w-0 flex-1">
            <span className="flex min-w-0 items-center gap-2">
              <span
                className={cn(
                  "shrink-0 border px-1.5 py-0.5 font-mono text-[10px] font-semibold uppercase tracking-[0.06em]",
                  type.labelClass
                )}
              >
                {node.typeLabel}
              </span>
              <span className={cn("font-mono text-[10px]", status.textClass)}>
                {status.label}
              </span>
              <span className="ml-auto shrink-0 font-mono text-[10px] text-(--muted)">
                {formatTraceDuration(node.duration)}
              </span>
            </span>
            <span className="mt-1.5 block truncate text-[14px] font-semibold text-(--foreground)">
              {node.name}
            </span>
            <span className="mt-1 flex min-w-0 items-center gap-2 font-mono text-[10px] text-(--muted)">
              <span className="truncate">{node.service}</span>
              <span className="text-(--muted-deep)">·</span>
              <span className="truncate">{node.id}</span>
            </span>
            {node.error ? (
              <span className="mt-2 block truncate border-l-2 border-[#ef4444] pl-2 font-mono text-[11px] text-[#ff8b86]">
                {node.error}
              </span>
            ) : null}
          </span>

          {retryable ? (
            <span className="relative z-10 shrink-0">
              <Button
                onClick={(event) => {
                  event.stopPropagation();
                  onRetry();
                }}
                variant="danger"
              >
                <RefreshCcw size={13} />
                Retry
              </Button>
            </span>
          ) : null}
        </span>
      </div>
    </div>
  );
}

const nodeStyle: Record<
  RuntimeNodeType,
  {
    icon: ComponentType<{ size?: number; strokeWidth?: number }>;
    iconClass: string;
    cardClass: string;
    labelClass: string;
  }
> = {
  event: {
    cardClass: "border-sky-300/20",
    icon: Mail,
    iconClass: "border-dashed border-sky-300/45 text-sky-200",
    labelClass: "border-sky-300/30 bg-sky-300/10 text-sky-200",
  },
  external: {
    cardClass: "border-rose-300/28",
    icon: Cloud,
    iconClass:
      "border-rose-300/55 text-rose-200 shadow-[0_0_18px_rgba(244,63,94,0.12)]",
    labelClass: "border-rose-300/35 bg-rose-300/10 text-rose-200",
  },
  function: {
    cardClass: "border-emerald-300/20",
    icon: Workflow,
    iconClass: "border-emerald-300/40 text-emerald-200",
    labelClass: "border-emerald-300/30 bg-emerald-300/10 text-emerald-200",
  },
  request: {
    cardClass: "border-[color-mix(in_srgb,var(--accent)_24%,transparent)]",
    icon: Route,
    iconClass:
      "border-[color-mix(in_srgb,var(--accent)_48%,transparent)] text-(--accent)",
    labelClass:
      "border-[color-mix(in_srgb,var(--accent)_34%,transparent)] bg-(--accent-soft) text-(--accent)",
  },
  worker: {
    cardClass:
      "border-amber-300/24 shadow-[inset_0_0_0_1px_rgba(251,191,36,0.06)]",
    icon: ServerCog,
    iconClass: "border-double border-amber-300/45 text-amber-200",
    labelClass: "border-amber-300/30 bg-amber-300/10 text-amber-200",
  },
};

const statusStyle: Record<
  ReturnType<typeof runtimeStatusIntent>,
  {
    icon: ComponentType<{ size?: number; strokeWidth?: number }>;
    label: string;
    badgeClass: string;
    textClass: string;
  }
> = {
  dead: {
    badgeClass: "bg-[#ef4444] text-white",
    icon: XCircle,
    label: "dead",
    textClass: "text-[#ff8b86]",
  },
  failed: {
    badgeClass: "bg-amber-400 text-black",
    icon: AlertTriangle,
    label: "failed",
    textClass: "text-amber-300",
  },
  retrying: {
    badgeClass: "bg-blue-300 text-black",
    icon: RefreshCcw,
    label: "retrying",
    textClass: "text-blue-300",
  },
  running: {
    badgeClass:
      "animate-pulse bg-cyan-300 text-black shadow-[0_0_14px_rgba(103,232,249,0.5)]",
    icon: Play,
    label: "running",
    textClass: "text-cyan-300",
  },
  success: {
    badgeClass: "bg-emerald-400 text-black",
    icon: Check,
    label: "success",
    textClass: "text-emerald-300",
  },
};
