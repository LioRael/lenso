import type { RuntimeHeatmap } from "../../hooks/use-runtime-queries";
import { cn } from "../../lib/cn";
import { EmptyState } from "../ui/empty-state";
import { RuntimeViewHeader } from "./runtime-view-header";

export function HeatmapView({
  heatmap,
  loading,
  queryError,
}: {
  heatmap: RuntimeHeatmap | undefined;
  loading: boolean;
  queryError: Error | null;
}) {
  if (loading) {
    return (
      <div className="isolate flex h-full min-w-0 flex-col overflow-hidden bg-(--background)">
        <RuntimeViewHeader
          meta="loading"
          summary="Backend runtime heatmap"
          title="Heatmap"
        />
        <div className="grid grid-cols-[repeat(20,minmax(0,1fr))] gap-0.5 p-3">
          {Array.from({ length: 120 }, (_, index) => (
            <div
              className="aspect-5/4 rounded-[1px] border border-(--border-subtle) bg-(--elevated)"
              key={index}
            />
          ))}
        </div>
      </div>
    );
  }

  if (queryError) {
    return (
      <div className="isolate flex h-full min-w-0 flex-col overflow-hidden bg-(--background)">
        <RuntimeViewHeader
          meta="error"
          summary="Backend runtime heatmap"
          title="Heatmap"
        />
        <EmptyState className="h-full bg-(--surface)">
          <EmptyState.Title>Heatmap unavailable</EmptyState.Title>
          <EmptyState.Description>{queryError.message}</EmptyState.Description>
        </EmptyState>
      </div>
    );
  }

  if (!heatmap || heatmap.cells.length === 0) {
    return (
      <div className="isolate flex h-full min-w-0 flex-col overflow-hidden bg-(--background)">
        <RuntimeViewHeader
          meta={heatmap ? `${heatmap.bucketSeconds}s buckets` : "no data"}
          summary="Backend runtime heatmap"
          title="Heatmap"
        />
        <EmptyState className="h-full bg-(--surface)">
          <EmptyState.Title>No runtime heatmap data</EmptyState.Title>
          <EmptyState.Description>
            The backend returned an empty heatmap for the current runtime
            window.
          </EmptyState.Description>
        </EmptyState>
      </div>
    );
  }

  const maxCount = Math.max(1, ...heatmap.cells.map((cell) => cell.totalCount));

  return (
    <div className="isolate flex h-full min-w-0 flex-col overflow-hidden bg-(--background)">
      <RuntimeViewHeader
        meta={`${heatmap.bucketSeconds}s buckets`}
        summary={`${heatmap.cells.length} backend cells`}
        title="Heatmap"
      />
      <div className="min-h-0 flex-1 overflow-auto bg-(--background) p-3">
        <div className="grid grid-cols-[repeat(20,minmax(0,1fr))] gap-0.5">
          {heatmap.cells.map((cell, index) => (
            <div
              className={cn(
                "relative aspect-5/4 rounded-[1px] border border-(--border-subtle) transition hover:z-1 hover:border-(--secondary)",
                cell.errorCount > 0 || cell.deadCount > 0
                  ? "bg-[#ef4444]/85"
                  : cell.avgDurationMs && cell.avgDurationMs > 1000
                    ? "bg-[color-mix(in_srgb,var(--accent)_75%,transparent)]"
                    : cell.avgDurationMs && cell.avgDurationMs > 200
                      ? "bg-[#22c55e]/55"
                      : "bg-[#3b82f6]/35"
              )}
              key={`${cell.bucketStart}:${cell.service}:${cell.nodeType}:${index}`}
              style={{
                opacity: Math.max(0.28, cell.totalCount / maxCount),
              }}
              title={`${cell.service} · ${cell.nodeType} · ${cell.totalCount} executions`}
            />
          ))}
        </div>
      </div>
    </div>
  );
}
