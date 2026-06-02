import { Grid3X3 } from "lucide-react";

import type { RuntimeStory } from "../../data/mock-runtime";
import { buildRuntimeStory } from "../../lib/story";
import { RuntimeViewHeader } from "./runtime-view-header";

export function HeatmapPlaceholderView({ story }: { story: RuntimeStory }) {
  const storySummary = buildRuntimeStory(story);

  return (
    <div className="grid h-full min-h-0 grid-rows-[auto_minmax(0,1fr)] overflow-hidden bg-(--background)">
      <RuntimeViewHeader
        meta={`${storySummary.services.length} services`}
        summary={`Runtime pressure map for ${storySummary.title}`}
        title="Heatmap"
      />
      <div className="grid min-h-0 place-items-center overflow-hidden p-4">
        <div className="w-full max-w-xl border border-(--border-subtle) bg-(--surface) p-4 shadow-(--elevation-card)">
          <div className="flex items-center gap-2 text-(--foreground)">
            <span className="grid size-8 place-items-center tint tint-warning">
              <Grid3X3 size={15} />
            </span>
            <div className="min-w-0">
              <h2 className="text-[14px] font-semibold">Heatmap coming soon</h2>
              <p className="mt-0.5 font-mono text-[11px] text-(--muted)">
                Runtime pressure map for {storySummary.title}
              </p>
            </div>
          </div>
          <div className="mt-4 grid gap-2 font-mono text-[11px] text-(--muted)">
            {storySummary.services.slice(0, 5).map((service, index) => (
              <div
                className="grid grid-cols-[minmax(0,1fr)_120px] items-center gap-3"
                key={service}
              >
                <span className="truncate">{service}</span>
                <span className="h-2 bg-[linear-gradient(90deg,rgba(251,191,36,0.18),rgba(239,68,68,0.42))]">
                  <span
                    className="block h-full bg-[color-mix(in_srgb,var(--warning)_60%,transparent)]"
                    style={{ width: `${Math.max(24, 86 - index * 12)}%` }}
                  />
                </span>
              </div>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}
