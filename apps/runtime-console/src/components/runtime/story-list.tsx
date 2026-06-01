import { AlertCircle, Boxes, Clock, Search } from "lucide-react";

import type { RuntimeStory } from "../../data/mock-runtime";
import { cn } from "../../lib/cn";
import { formatRuntimeDuration, statusColor } from "../../lib/runtime-style";
import { buildRuntimeStory } from "../../lib/story";

export function StoryList({
  query,
  selectedStoryId,
  setQuery,
  stories,
  onSelect,
}: {
  stories: RuntimeStory[];
  selectedStoryId: string | null;
  query: string;
  setQuery: (query: string) => void;
  onSelect: (story: RuntimeStory) => void;
}) {
  return (
    <aside className="grid h-full min-h-0 min-w-0 grid-rows-[auto_auto_auto_minmax(0,1fr)] overflow-hidden bg-(--background)">
      <div className="flex min-h-10 items-center justify-between gap-2 border-b border-(--border-subtle) bg-(--surface) px-3 py-2">
        <div>
          <h2 className="font-mono text-sm font-semibold tracking-tight text-(--foreground)">
            Stories
          </h2>
          <p className="font-mono text-xs text-(--muted)">
            {stories.length} correlations
          </p>
        </div>
      </div>
      <div className="flex h-8 items-center gap-2 border-b border-(--border-subtle) px-3 text-(--muted)">
        <Search size={12} />
        <input
          aria-label="Search stories"
          className="mono w-full bg-transparent text-xs text-(--foreground) outline-hidden placeholder:text-(--muted)"
          onChange={(event) => setQuery(event.target.value)}
          placeholder="filter story / service / correlation..."
          value={query}
        />
      </div>
      <div className="grid h-6 grid-cols-[12px_minmax(0,1fr)_58px] items-center gap-2 border-b border-(--border-subtle) px-3 font-mono text-[10px] font-semibold uppercase tracking-[0.06em] text-(--muted)">
        <span />
        <span>story</span>
        <span className="text-right">state</span>
      </div>
      <div className="min-h-0 overflow-auto">
        {stories.map((story) => {
          const storySummary = buildRuntimeStory(story);
          const isError =
            storySummary.status === "failed" || storySummary.status === "dead";

          return (
            <button
              className={cn(
                "w-full border-b border-(--border-subtle) px-3 py-2 text-left transition",
                selectedStoryId === story.id
                  ? "bg-(--accent-soft) shadow-[inset_2px_0_0_var(--accent)]"
                  : "hover:bg-(--elevated)"
              )}
              key={story.id}
              onClick={() => onSelect(story)}
              type="button"
            >
              <div className="mb-1 flex items-center gap-1.5">
                <span
                  className="size-1.5 shrink-0 rounded-full"
                  style={{
                    backgroundColor: statusColor(storySummary.status),
                    boxShadow: isError
                      ? `0 0 8px ${statusColor(storySummary.status)}`
                      : undefined,
                  }}
                />
                <span className="min-w-0 flex-1 truncate text-[13px] font-semibold text-(--foreground)">
                  {storySummary.title}
                </span>
                <span
                  className={cn(
                    "font-mono text-[10px] uppercase",
                    isError ? "text-[#ff8b86]" : "text-(--muted)"
                  )}
                >
                  {storySummary.status}
                </span>
              </div>

              <div className="mb-1.5 truncate font-mono text-[10px] text-(--muted)">
                {storySummary.correlationId}
              </div>

              <div className="mb-1.5 flex flex-wrap items-center gap-1.5 font-mono text-[10px] text-(--secondary)">
                <Metric icon={<Clock size={10} />}>
                  {formatRuntimeDuration(storySummary.duration)}
                </Metric>
                <Metric icon={<Boxes size={10} />}>
                  {storySummary.nodeCount} nodes
                </Metric>
                <Metric
                  className={
                    storySummary.errorCount > 0 ? "text-[#ff8b86]" : ""
                  }
                  icon={<AlertCircle size={10} />}
                >
                  {storySummary.errorCount} errors
                </Metric>
              </div>

              <div className="truncate font-mono text-[10px] leading-4 text-(--secondary)">
                {storySummary.patternLabel || "No execution pattern"}
              </div>

              <div className="mt-1 flex min-w-0 flex-wrap gap-1">
                {storySummary.services.slice(0, 4).map((service) => (
                  <span
                    className="max-w-24 truncate border border-(--border-subtle) bg-(--elevated) px-1 py-0.5 font-mono text-[9px] text-(--muted)"
                    key={service}
                  >
                    {service}
                  </span>
                ))}
                {storySummary.services.length > 4 ? (
                  <span className="border border-(--border-subtle) bg-(--elevated) px-1 py-0.5 font-mono text-[9px] text-(--muted)">
                    +{storySummary.services.length - 4}
                  </span>
                ) : null}
              </div>

              {storySummary.rootError ? (
                <div className="mt-1.5 truncate border-l-2 border-[#ef4444] pl-2 font-mono text-[10px] leading-4 text-[#ff8b86]">
                  {storySummary.rootError}
                </div>
              ) : null}
            </button>
          );
        })}
      </div>
    </aside>
  );
}

function Metric({
  children,
  className,
  icon,
}: {
  children: React.ReactNode;
  className?: string;
  icon: React.ReactNode;
}) {
  return (
    <span className={cn("inline-flex items-center gap-1", className)}>
      {icon}
      {children}
    </span>
  );
}
