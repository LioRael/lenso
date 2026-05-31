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
        "inline-flex min-h-[23px] w-fit items-center gap-1.5 rounded-full border px-2.5 text-[11px] font-semibold",
        tone.pill
      )}
    >
      <span
        className={cn(
          "size-1.5 rounded-full shadow-[0_0_16px_currentColor]",
          tone.dot
        )}
      />
      {status}
    </span>
  );
}

const statusTone: Record<RuntimeStatus, { pill: string; dot: string }> = {
  pending: {
    pill: "border-slate-400/25 bg-slate-400/10 text-slate-300",
    dot: "bg-slate-400 text-slate-400",
  },
  processing: {
    pill: "border-blue-300/30 bg-blue-300/10 text-blue-200",
    dot: "bg-blue-300 text-blue-300",
  },
  running: {
    pill: "border-cyan-300/30 bg-cyan-300/10 text-cyan-200",
    dot: "bg-cyan-300 text-cyan-300",
  },
  published: {
    pill: "border-emerald-300/30 bg-emerald-300/10 text-emerald-200",
    dot: "bg-emerald-300 text-emerald-300",
  },
  completed: {
    pill: "border-emerald-300/30 bg-emerald-300/10 text-emerald-200",
    dot: "bg-emerald-300 text-emerald-300",
  },
  failed: {
    pill: "border-amber-300/30 bg-amber-300/10 text-amber-200",
    dot: "bg-amber-300 text-amber-300",
  },
  dead: {
    pill: "border-rose-300/35 bg-rose-300/10 text-rose-200",
    dot: "bg-rose-300 text-rose-300",
  },
};
