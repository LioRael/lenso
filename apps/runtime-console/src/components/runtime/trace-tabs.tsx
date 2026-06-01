import { Flame, GitBranch, Grid3X3, List } from "lucide-react";

import { cn } from "../../lib/cn";

export type TraceViewMode = "waterfall" | "flame" | "heatmap" | "flow";

const labels: Array<{
  id: TraceViewMode;
  label: string;
  icon: React.ComponentType<{
    className?: string;
    size?: number;
    strokeWidth?: number;
  }>;
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
    <div className="flex h-8 items-end border-b border-[var(--border-subtle)] bg-[color-mix(in_srgb,var(--surface)_76%,var(--background))] px-3">
      <div className="flex h-full items-end gap-3">
        {labels.map(({ icon: Icon, id, label }) => (
          <button
            className={cn(
              "relative flex h-8 items-center gap-1.5 border-b border-transparent px-0.5 font-mono text-[11px] transition",
              mode === id
                ? "border-[var(--accent)] font-semibold text-[var(--foreground)]"
                : "text-[var(--muted)] hover:border-[var(--border)] hover:text-[var(--secondary)]"
            )}
            key={id}
            onClick={() => onChange(id)}
          >
            <Icon
              {...(mode === id
                ? { className: "text-[var(--accent)]" }
                : {})}
              size={12}
              strokeWidth={1.75}
            />
            {label}
          </button>
        ))}
      </div>
    </div>
  );
}
