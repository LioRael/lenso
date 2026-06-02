import {
  Bot,
  Braces,
  Cloud,
  GitBranch,
  Mail,
  MousePointer2,
  RotateCcw,
  Route,
  Workflow,
} from "lucide-react";

import type { TimelineItem } from "../../data/mock-runtime";
import { cn } from "../../lib/cn";
import { duration, time } from "../../lib/format";
import { Button } from "../ui/button";
import { StatusPill } from "./status-pill";

type TimelineNodeProps = {
  index: number;
  item: TimelineItem;
  onOpen: () => void;
  onRetry: () => void;
};

export function TimelineNode({
  index,
  item,
  onOpen,
  onRetry,
}: TimelineNodeProps) {
  const retryable = item.status === "failed" || item.status === "dead";

  return (
    <article className="relative grid grid-cols-[46px_34px_minmax(0,1fr)] gap-3 pb-3 last:pb-0 max-sm:grid-cols-[34px_minmax(0,1fr)]">
      <div className="mono pt-4 text-right text-xs text-(--muted) max-sm:hidden">
        {String(index + 1).padStart(2, "0")}
      </div>
      <div className="relative flex justify-center after:absolute after:-bottom-3.5 after:top-8.5 after:w-px after:bg-gradient-to-b after:from-(--border) after:to-transparent last:after:hidden">
        <span
          className={cn(
            "relative z-10 mt-2 grid size-8.5 place-items-center rounded-lg border shadow-(--elevation-raised)",
            nodeTone[item.status]
          )}
        >
          <NodeIcon type={item.type} />
        </span>
      </div>
      <button
        className={cn(
          "w-full rounded-lg border border-(--border-subtle) bg-(--elevated) p-3.5 text-left text-(--foreground) transition hover:-translate-y-px hover:border-(--border) hover:bg-(--hover)",
          retryable && "border-current/20",
          item.status === "dead" && "tint-soft tint-error tint-text",
          item.status === "failed" && "tint-soft tint-warning tint-text"
        )}
        onClick={onOpen}
      >
        <div className="flex justify-between gap-3.5">
          <div className="min-w-0">
            <div className="mono text-[11px] uppercase text-(--muted)">
              {item.type}
            </div>
            <h3 className="mt-1 truncate text-[15px] font-bold text-(--foreground)">
              {item.name}
            </h3>
          </div>
          <StatusPill status={item.status} />
        </div>
        <div className="mt-3 flex flex-wrap gap-x-3.5 gap-y-2 text-xs text-(--secondary)">
          <span>{time(item.createdAt)}</span>
          <span>{duration(item.startedAt, item.completedAt)}</span>
          <span>
            attempts {item.attempts}/{item.maxAttempts}
          </span>
          <span className="mono">{item.id}</span>
        </div>
        {item.lastError ? (
          <div className="mono mt-3 border-l-2 tint-border tint-error pl-2.5 text-xs tint-text">
            {item.lastError}
          </div>
        ) : null}
        {retryable ? (
          <div className="mt-3 flex justify-end">
            <Button
              onClick={(event) => {
                event.stopPropagation();
                onRetry();
              }}
              variant="danger"
            >
              <RotateCcw size={15} />
              Retry
            </Button>
          </div>
        ) : null}
      </button>
    </article>
  );
}

export function TimelineNodeIcon({ type }: { type: TimelineItem["type"] }) {
  return <NodeIcon type={type} />;
}

const nodeTone: Record<TimelineItem["status"], string> = {
  completed: "tint tint-success",
  dead: "tint tint-error",
  failed: "tint tint-warning",
  pending: "border-(--border) bg-(--elevated) text-(--secondary)",
  processing: "tint tint-info",
  published: "tint tint-success",
  running: "tint tint-info",
};

function NodeIcon({ type }: { type: TimelineItem["type"] }) {
  switch (type) {
    case "http_request": {
      return <Route size={15} />;
    }
    case "command": {
      return <Braces size={15} />;
    }
    case "outbox_event": {
      return <Mail size={15} />;
    }
    case "function_run": {
      return <Workflow size={15} />;
    }
    case "flow_step": {
      return <GitBranch size={15} />;
    }
    case "agent_tool_call": {
      return <Bot size={15} />;
    }
    case "external_provider_call": {
      return <Cloud size={15} />;
    }
    default: {
      return <MousePointer2 size={15} />;
    }
  }
}
