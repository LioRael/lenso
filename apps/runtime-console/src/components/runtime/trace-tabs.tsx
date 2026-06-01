import { Flame, GitBranch, Grid3X3, List } from "lucide-react";

import { cn } from "../../lib/cn";

export type TraceViewMode = "waterfall" | "flame" | "heatmap" | "flow";

const labels: Array<{
  id: TraceViewMode;
  label: string;
  icon: React.ComponentType<{ size?: number }>;
}> = [
  { id: "waterfall", label: "Waterfall", icon: List },
  { id: "flame", label: "Flame", icon: Flame },
  { id: "heatmap", label: "Heatmap", icon: Grid3X3 },
  { id: "flow", label: "Flow", icon: GitBranch },
];

export function TraceTabs({
  mode,
  onChange,
}: {
  mode: TraceViewMode;
  onChange: (mode: TraceViewMode) => void;
}) {
  return (
    <div className="flex h-8 items-center border-b border-[var(--border-subtle)] bg-[var(--surface)] px-3">
      <div className="inline-flex items-center gap-px rounded-[2px] border border-[var(--border-subtle)] bg-[var(--background)] p-0.5">
        {labels.map(({ icon: Icon, id, label }) => (
          <button
            className={cn(
              "flex h-6 items-center gap-1.5 rounded-[2px] px-2.5 font-mono text-[10px] transition",
              mode === id
                ? "bg-[var(--accent)] font-semibold text-[var(--inverse)] shadow-[0_0_8px_color-mix(in_srgb,var(--accent)_16%,transparent)]"
                : "text-[var(--muted)] hover:bg-[var(--elevated)] hover:text-[var(--foreground)]"
            )}
            key={id}
            onClick={() => onChange(id)}
          >
            <Icon size={12} />
            {label}
          </button>
        ))}
      </div>
    </div>
  );
}
