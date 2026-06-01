import { Grid3X3 } from "lucide-react";

import type { TraceRun } from "../../data/mock-runtime";
import { buildRuntimeStory } from "../../lib/story";

export function HeatmapPlaceholderView({ trace }: { trace: TraceRun }) {
  const story = buildRuntimeStory(trace);

  return (
    <div className="grid h-full min-h-0 place-items-center overflow-hidden bg-(--background) p-4">
      <div className="w-full max-w-xl border border-(--border-subtle) bg-(--surface) p-4 shadow-[0_14px_34px_var(--shadow-soft)]">
        <div className="flex items-center gap-2 text-(--foreground)">
          <span className="grid size-8 place-items-center border border-amber-300/30 bg-amber-300/10 text-amber-200">
            <Grid3X3 size={15} />
          </span>
          <div className="min-w-0">
            <h2 className="text-[14px] font-semibold">Heatmap coming soon</h2>
            <p className="mt-0.5 font-mono text-[11px] text-(--muted)">
              Runtime pressure map for {story.title}
            </p>
          </div>
        </div>
        <div className="mt-4 grid gap-2 font-mono text-[11px] text-(--muted)">
          {story.services.slice(0, 5).map((service, index) => (
            <div
              className="grid grid-cols-[minmax(0,1fr)_120px] items-center gap-3"
              key={service}
            >
              <span className="truncate">{service}</span>
              <span className="h-2 bg-[linear-gradient(90deg,rgba(251,191,36,0.18),rgba(239,68,68,0.42))]">
                <span
                  className="block h-full bg-amber-300/60"
                  style={{ width: `${Math.max(24, 86 - index * 12)}%` }}
                />
              </span>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
