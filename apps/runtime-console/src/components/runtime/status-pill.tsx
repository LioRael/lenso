import type { RuntimeStatus } from "../../data/mock-runtime";
import { cn } from "../../lib/cn";

type StatusPillProps = {
  status: RuntimeStatus;
};

export function StatusPill({ status }: StatusPillProps) {
  const tone = statusTone[status];
  return (
    <span
      className={cn(
        "status-pill inline-flex min-h-[23px] w-fit items-center gap-1.5 rounded-full border px-2.5 text-[11px] font-semibold",
        tone.className
      )}
    >
      <span className="status-pill-dot size-1.5 rounded-full shadow-[0_0_16px_currentColor]" />
      {status}
    </span>
  );
}

const statusTone: Record<RuntimeStatus, { className: string }> = {
  pending: {
    className: "status-pill-pending",
  },
  processing: {
    className: "status-pill-processing",
  },
  running: {
    className: "status-pill-running",
  },
  published: {
    className: "status-pill-published",
  },
  completed: {
    className: "status-pill-completed",
  },
  failed: {
    className: "status-pill-failed",
  },
  dead: {
    className: "status-pill-dead",
  },
};
