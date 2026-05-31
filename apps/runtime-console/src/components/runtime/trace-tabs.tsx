import { cn } from "../../lib/cn";

export type TraceViewMode = "waterfall" | "flame" | "heatmap" | "flow";

const labels: TraceViewMode[] = ["waterfall", "flame", "heatmap", "flow"];

export function TraceTabs({
  mode,
  onChange,
}: {
  mode: TraceViewMode;
  onChange: (mode: TraceViewMode) => void;
}) {
  return (
    <div className="flex h-[29px] items-center border-b border-white/10 bg-[#07080a]">
      {labels.map((label) => (
        <button
          className={cn(
            "h-full border-r border-white/10 px-3 font-mono text-[10px] uppercase tracking-[0.03em] text-slate-600 hover:bg-white/[0.04] hover:text-slate-200",
            mode === label && "bg-cyan-300/[0.06] text-cyan-200"
          )}
          key={label}
          onClick={() => onChange(label)}
        >
          {label}
        </button>
      ))}
    </div>
  );
}
