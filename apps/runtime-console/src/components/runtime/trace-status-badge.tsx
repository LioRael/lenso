import {
  Activity,
  AlertTriangle,
  CheckCircle2,
  CircleDot,
  Clock3,
  LoaderCircle,
  OctagonX,
} from "lucide-react";

import type { RuntimeStatus } from "../../data/mock-runtime";
import { cn } from "../../lib/cn";

type TraceStatusBadgeProps = {
  className?: string;
  status: RuntimeStatus;
  variant?: "default" | "compact" | "table";
};

export function TraceStatusBadge({
  className,
  status,
  variant = "default",
}: TraceStatusBadgeProps) {
  const tone = traceStatusTone[status];
  const StatusIcon = tone.icon;
  const showIcon = variant !== "table";

  return (
    <span
      className={cn(
        traceStatusBadgeBaseClassName,
        tone.className,
        variant === "compact" && traceStatusBadgeCompactClassName,
        variant === "table" && traceStatusBadgeTableClassName,
        className
      )}
      title={tone.label}
    >
      {showIcon ? (
        <StatusIcon
          className={cn(
            "shrink-0",
            variant === "compact" ? "size-2.5" : "size-3"
          )}
          strokeWidth={2.2}
        />
      ) : null}
      <span className="truncate">{tone.label}</span>
    </span>
  );
}

export const traceStatusBadgeBaseClassName =
  "trace-status-badge inline-flex min-h-5 w-fit max-w-full items-center gap-1 rounded-xs border px-1.5 font-mono text-[10px] font-semibold uppercase leading-none tracking-[0.06em]";

export const traceStatusBadgeCompactClassName = "min-h-4.5 px-1 text-[9px]";

export const traceStatusBadgeTableClassName =
  "min-h-4.5 w-[72px] justify-center px-1 text-[9px] tracking-[0.08em]";

const traceStatusTone: Record<
  RuntimeStatus,
  { className: string; icon: typeof Clock3; label: string }
> = {
  pending: {
    className: "trace-status-pending",
    icon: Clock3,
    label: "pending",
  },
  processing: {
    className: "trace-status-processing",
    icon: LoaderCircle,
    label: "processing",
  },
  running: {
    className: "trace-status-running",
    icon: Activity,
    label: "running",
  },
  published: {
    className: "trace-status-published",
    icon: CircleDot,
    label: "published",
  },
  completed: {
    className: "trace-status-completed",
    icon: CheckCircle2,
    label: "completed",
  },
  failed: {
    className: "trace-status-failed",
    icon: AlertTriangle,
    label: "failed",
  },
  dead: {
    className: "trace-status-dead",
    icon: OctagonX,
    label: "dead",
  },
};
