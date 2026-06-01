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
      <div className="mono pt-4 text-right text-xs text-slate-600 max-sm:hidden">
        {String(index + 1).padStart(2, "0")}
      </div>
      <div className="relative flex justify-center after:absolute after:-bottom-3.5 after:top-8.5 after:w-px after:bg-gradient-to-b after:from-white/15 after:to-white/2 last:after:hidden">
        <span
          className={cn(
            "relative z-10 mt-2 grid size-8.5 place-items-center rounded-lg border shadow-[0_0_28px_rgba(0,0,0,0.36)]",
            nodeTone[item.status]
          )}
        >
          <NodeIcon type={item.type} />
        </span>
      </div>
      <button
        className={cn(
          "w-full rounded-lg border border-white/10 bg-white/[0.028] p-3.5 text-left text-slate-100 transition hover:-translate-y-px hover:border-blue-300/20 hover:bg-blue-300/5.5",
          retryable && "border-current/20",
          item.status === "dead" && "bg-rose-300/[0.045] text-rose-200",
          item.status === "failed" && "bg-amber-300/[0.045] text-amber-200"
        )}
        onClick={onOpen}
      >
        <div className="flex justify-between gap-3.5">
          <div className="min-w-0">
            <div className="mono text-[11px] uppercase text-slate-600">
              {item.type}
            </div>
            <h3 className="mt-1 truncate text-[15px] font-bold text-slate-100">
              {item.name}
            </h3>
          </div>
          <StatusPill status={item.status} />
        </div>
        <div className="mt-3 flex flex-wrap gap-x-3.5 gap-y-2 text-xs text-slate-400">
          <span>{time(item.createdAt)}</span>
          <span>{duration(item.startedAt, item.completedAt)}</span>
          <span>
            attempts {item.attempts}/{item.maxAttempts}
          </span>
          <span className="mono">{item.id}</span>
        </div>
        {item.lastError ? (
          <div className="mono mt-3 border-l-2 border-rose-300 pl-2.5 text-xs text-rose-100">
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
  completed: "border-emerald-300/35 bg-emerald-300/10 text-emerald-200",
  dead: "border-rose-300/40 bg-rose-300/10 text-rose-200",
  failed: "border-amber-300/40 bg-amber-300/10 text-amber-200",
  pending: "border-slate-400/25 bg-slate-400/10 text-slate-300",
  processing: "border-blue-300/35 bg-blue-300/10 text-blue-200",
  published: "border-emerald-300/35 bg-emerald-300/10 text-emerald-200",
  running: "border-cyan-300/35 bg-cyan-300/10 text-cyan-200",
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
