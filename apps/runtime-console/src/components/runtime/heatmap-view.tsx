import { X } from "lucide-react";
import { useMemo, useState } from "react";

import type { ExecutionNode, RuntimeStory } from "../../data/mock-runtime";
import type { RuntimeHeatmap } from "../../hooks/use-runtime-queries";
import { cn } from "../../lib/cn";
import { formatRuntimeDuration, statusColor } from "../../lib/runtime-style";
import { Button } from "../ui/button";
import { EmptyState } from "../ui/empty-state";
import { heatmapCellKey, resolveHeatmapCellNodes } from "./heatmap-model";
import { RuntimeViewHeader } from "./runtime-view-header";

export function HeatmapView({
  heatmap,
  loading,
  selectedNodeId,
  story,
  queryError,
  onSelectNode,
}: {
  heatmap: RuntimeHeatmap | undefined;
  loading: boolean;
  selectedNodeId?: string | null;
  story?: RuntimeStory;
  queryError: Error | null;
  onSelectNode?: (node: ExecutionNode) => void;
}) {
  const [selectedCellKey, setSelectedCellKey] = useState<string | null>(null);
  const drilldownEnabled = Boolean(story && onSelectNode);
  const selectedCell = useMemo(() => {
    if (!(heatmap && story && selectedCellKey)) {
      return null;
    }

    const index = heatmap.cells.findIndex(
      (cell, cellIndex) => heatmapCellKey(cell, cellIndex) === selectedCellKey
    );
    const cell = heatmap.cells[index];
    if (!cell) {
      return null;
    }

    return {
      cell,
      key: selectedCellKey,
      nodes: resolveHeatmapCellNodes({ cell, story }),
    };
  }, [heatmap, selectedCellKey, story]);

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
      <div
        className={cn(
          "grid min-h-0 flex-1 overflow-hidden",
          drilldownEnabled && selectedCell
            ? "grid-cols-[minmax(0,1fr)_minmax(0,280px)] max-xl:grid-cols-1"
            : "grid-cols-1"
        )}
      >
        <div className="min-h-0 overflow-auto bg-(--background) p-3">
          <div className="grid grid-cols-[repeat(20,minmax(0,1fr))] gap-0.5">
            {heatmap.cells.map((cell, index) => {
              const key = heatmapCellKey(cell, index);
              const nodes =
                story === undefined
                  ? []
                  : resolveHeatmapCellNodes({ cell, story });
              const selected = key === selectedCellKey;
              const directlySelectable = drilldownEnabled && nodes.length === 1;
              const aggregateSelectable = drilldownEnabled && nodes.length > 1;

              return (
                <button
                  aria-label={`${cell.service} ${cell.nodeType} heatmap cell with ${cell.totalCount} executions`}
                  className={cn(
                    "relative aspect-5/4 rounded-[1px] border border-(--border-subtle) transition hover:z-1 hover:border-(--secondary) focus-visible:z-1 focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-(--accent)",
                    heatmapCellColor(cell),
                    selected &&
                      "border-(--accent) shadow-[0_0_0_1px_var(--accent)]",
                    drilldownEnabled ? "cursor-pointer" : "cursor-default"
                  )}
                  disabled={!drilldownEnabled}
                  key={key}
                  onClick={() => {
                    if (!drilldownEnabled) {
                      return;
                    }
                    if (nodes.length === 1 && nodes[0]) {
                      onSelectNode?.(nodes[0]);
                      setSelectedCellKey(null);
                      return;
                    }
                    setSelectedCellKey(selected ? null : key);
                  }}
                  style={{
                    opacity: Math.max(0.28, cell.totalCount / maxCount),
                  }}
                  title={`${cell.service} · ${cell.nodeType} · ${cell.totalCount} executions`}
                  type="button"
                >
                  {aggregateSelectable ? (
                    <span className="absolute right-0.5 bottom-0.5 rounded-[1px] bg-(--background)/80 px-1 font-mono text-[9px] text-(--foreground)">
                      {nodes.length}
                    </span>
                  ) : null}
                  {directlySelectable &&
                  selectedNodeId &&
                  selectedNodeId === nodes[0]?.id ? (
                    <span className="absolute inset-1 border border-(--background)" />
                  ) : null}
                </button>
              );
            })}
          </div>
        </div>
        {drilldownEnabled && selectedCell ? (
          <HeatmapCellInspector
            selected={selectedCell}
            selectedNodeId={selectedNodeId ?? null}
            onClear={() => setSelectedCellKey(null)}
            onSelectNode={(node) => onSelectNode?.(node)}
          />
        ) : null}
      </div>
    </div>
  );
}

function HeatmapCellInspector({
  selected,
  selectedNodeId,
  onClear,
  onSelectNode,
}: {
  selected: {
    cell: RuntimeHeatmap["cells"][number];
    key: string;
    nodes: ExecutionNode[];
  };
  selectedNodeId: string | null;
  onClear: () => void;
  onSelectNode: (node: ExecutionNode) => void;
}) {
  return (
    <aside className="min-h-0 overflow-hidden border-l border-(--border-subtle) bg-(--surface)">
      <div className="flex min-w-0 items-start gap-2 border-b border-(--border-subtle) px-3 py-2">
        <div className="min-w-0 flex-1">
          <div className="truncate font-mono text-[11px] font-semibold text-(--foreground)">
            {selected.cell.service} · {selected.cell.nodeType}
          </div>
          <div className="mt-0.5 truncate font-mono text-[10px] text-(--muted)">
            {selected.cell.totalCount} total · {selected.cell.errorCount} errors
          </div>
        </div>
        <Button
          aria-label="Clear heatmap cell selection"
          className="size-7 p-0"
          onClick={onClear}
          variant="ghost"
        >
          <X size={13} />
        </Button>
      </div>
      <div className="min-h-0 overflow-auto">
        {selected.nodes.length === 0 ? (
          <div className="p-3 font-mono text-[11px] text-(--muted)">
            No matching story nodes were found for this cell.
          </div>
        ) : (
          selected.nodes.map((node) => (
            <button
              aria-label={`Open heatmap node ${node.name}`}
              className={cn(
                "grid w-full min-w-0 gap-1 border-b border-(--border-subtle) px-3 py-2 text-left transition hover:bg-(--hover)",
                selectedNodeId === node.id &&
                  "bg-(--accent-soft) shadow-[inset_2px_0_0_var(--accent)]"
              )}
              key={node.id}
              onClick={() => onSelectNode(node)}
              type="button"
            >
              <span className="flex min-w-0 items-center gap-2">
                <span
                  className="size-1.5 shrink-0 rounded-full"
                  style={{ backgroundColor: statusColor(node.status) }}
                />
                <span className="truncate text-[12px] font-semibold text-(--foreground)">
                  {node.name}
                </span>
              </span>
              <span className="flex min-w-0 items-center gap-2 font-mono text-[10px] text-(--muted)">
                <span className="truncate">{node.service}</span>
                <span className="ml-auto shrink-0">
                  {formatRuntimeDuration(node.durationMs)}
                </span>
              </span>
            </button>
          ))
        )}
      </div>
    </aside>
  );
}

function heatmapCellColor(cell: RuntimeHeatmap["cells"][number]) {
  if (cell.errorCount > 0 || cell.deadCount > 0) {
    return "bg-[#ef4444]/85";
  }
  if (cell.avgDurationMs && cell.avgDurationMs > 1000) {
    return "bg-[color-mix(in_srgb,var(--accent)_75%,transparent)]";
  }
  if (cell.avgDurationMs && cell.avgDurationMs > 200) {
    return "bg-[#22c55e]/55";
  }
  return "bg-[#3b82f6]/35";
}
