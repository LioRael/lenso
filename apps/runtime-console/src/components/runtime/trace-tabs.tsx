import { Flame, GitBranch, Grid3X3, List, Workflow } from "lucide-react";

import { cn } from "../../lib/cn";
import { HorizontalTabScroll } from "./horizontal-tab-scroll";

export type TraceViewMode =
  | "story"
  | "waterfall"
  | "flame"
  | "heatmap"
  | "flow";

const labels: Array<{
  id: TraceViewMode;
  label: string;
  icon: React.ComponentType<{
    className?: string;
    size?: number;
    strokeWidth?: number;
  }>;
}> = [
  { id: "story", label: "Timeline", icon: Workflow },
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
    <div className="min-w-0 border-b border-(--border-subtle) bg-[color-mix(in_srgb,var(--surface)_76%,var(--background))]">
      <HorizontalTabScroll>
        <div className="flex h-full w-max min-w-full items-end gap-3 pl-3 pr-8">
          {labels.map(({ icon: Icon, id, label }) => (
            <button
              className={cn(
                "relative flex h-8 shrink-0 items-center gap-1.5 whitespace-nowrap border-b border-transparent px-0.5 font-mono text-[11px] transition",
                mode === id
                  ? "font-semibold text-(--foreground) shadow-[inset_0_-1px_0_var(--accent)]"
                  : "text-(--muted) hover:border-(--border) hover:text-(--secondary)"
              )}
              key={id}
              onClick={() => onChange(id)}
            >
              <Icon
                {...(mode === id ? { className: "text-(--accent)" } : {})}
                size={12}
                strokeWidth={1.75}
              />
              {label}
            </button>
          ))}
        </div>
      </HorizontalTabScroll>
    </div>
  );
}
