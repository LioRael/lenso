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

type RuntimeStatusBadgeProps = {
  className?: string;
  status: RuntimeStatus;
  variant?: "default" | "compact" | "label" | "table";
};

export function RuntimeStatusBadge({
  className,
  status,
  variant = "default",
}: RuntimeStatusBadgeProps) {
  const tone = runtimeStatusTone[status];
  const StatusIcon = tone.icon;
  const showIcon = variant !== "table";

  return (
    <span
      className={cn(
        runtimeStatusBadgeBaseClassName,
        tone.className,
        variant === "compact" && runtimeStatusBadgeCompactClassName,
        variant === "label" && runtimeStatusBadgeLabelClassName,
        variant === "table" && runtimeStatusBadgeTableClassName,
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

export const runtimeStatusBadgeBaseClassName =
  "runtime-status-badge inline-flex min-h-5 w-fit max-w-full items-center gap-1 rounded-xs border px-1.5 font-mono text-[10px] font-semibold uppercase leading-none tracking-[0.06em]";

export const runtimeStatusBadgeCompactClassName = "min-h-4.5 px-1 text-[9px]";

export const runtimeStatusBadgeLabelClassName =
  "runtime-status-label py-0.5 text-[10px] tracking-[0.08em]";

export const runtimeStatusBadgeTableClassName =
  "min-h-4.5 w-[72px] justify-center px-1 text-[9px] tracking-[0.08em]";

const runtimeStatusTone: Record<
  RuntimeStatus,
  { className: string; icon: typeof Clock3; label: string }
> = {
  pending: {
    className: "runtime-status-pending",
    icon: Clock3,
    label: "pending",
  },
  processing: {
    className: "runtime-status-processing",
    icon: LoaderCircle,
    label: "processing",
  },
  running: {
    className: "runtime-status-running",
    icon: Activity,
    label: "running",
  },
  published: {
    className: "runtime-status-published",
    icon: CircleDot,
    label: "published",
  },
  completed: {
    className: "runtime-status-completed",
    icon: CheckCircle2,
    label: "completed",
  },
  failed: {
    className: "runtime-status-failed",
    icon: AlertTriangle,
    label: "failed",
  },
  dead: {
    className: "runtime-status-dead",
    icon: OctagonX,
    label: "dead",
  },
};
