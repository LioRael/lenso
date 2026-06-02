import type { RuntimeStory, ExecutionNode } from "../../data/mock-runtime";
import { cn } from "../../lib/cn";
import {
  formatRuntimeDuration,
  serviceColor,
  statusColor,
} from "../../lib/runtime-style";
import { runtimeWaterfallTableHeaderClassName } from "./runtime-table-header";
import { RuntimeViewHeader } from "./runtime-view-header";
import {
  buildWaterfallRows,
  findExecutionNodeForWaterfallRow,
  waterfallTimelineEnd,
  type WaterfallRow,
  type WaterfallTimelineMarker,
} from "./waterfall-model";

export function WaterfallView({
  selectedNodeId,
  story,
  onSelectNode,
}: {
  story: RuntimeStory;
  selectedNodeId: string | null;
  onSelectNode: (node: ExecutionNode) => void;
}) {
  const rows = buildWaterfallRows(story);
  const timelineEnd = waterfallTimelineEnd(story);
  const unlinkedCount = rows.filter((row) => row.group === "unlinked").length;

  return (
    <div className="isolate flex h-full min-w-0 flex-col overflow-hidden bg-(--background)">
      <RuntimeViewHeader
        meta={`total ${formatRuntimeDuration(timelineEnd)}`}
        summary={`${rows.length} execution rows${unlinkedCount > 0 ? ` · ${unlinkedCount} unlinked` : ""}`}
        title="Waterfall"
      />
      <div className={runtimeWaterfallTableHeaderClassName}>
        <span>Node</span>
        <div className="grid min-w-0 grid-cols-5 overflow-hidden">
          {[0, 25, 50, 75, 100].map((tick) => (
            <span className="font-mono" key={tick}>
              {formatRuntimeDuration((timelineEnd * tick) / 100)}
            </span>
          ))}
        </div>
      </div>
      <div className="min-h-0 flex-1 overflow-auto">
        {rows.length === 0 ? (
          <div className="border-b border-(--border-subtle) p-4 font-mono text-xs text-(--muted)">
            No waterfall rows were returned for this story.
          </div>
        ) : null}
        {rows.map((row, index) => {
          const previousRow = rows[index - 1];
          const showUnlinkedHeader =
            row.group === "unlinked" && previousRow?.group !== "unlinked";
          return (
            <div key={row.id}>
              {showUnlinkedHeader ? (
                <div className="border-y border-(--border-subtle) bg-(--sidebar) px-3 py-1.5 font-mono text-[10px] font-semibold uppercase tracking-[0.08em] text-(--muted)">
                  Unlinked
                </div>
              ) : null}
              <WaterfallRowButton
                onSelectNode={onSelectNode}
                row={row}
                selectedNodeId={selectedNodeId}
                timelineEnd={timelineEnd}
              />
            </div>
          );
        })}
      </div>
    </div>
  );
}

function WaterfallRowButton({
  row,
  selectedNodeId,
  timelineEnd,
  onSelectNode,
}: {
  row: WaterfallRow;
  selectedNodeId: string | null;
  timelineEnd: number;
  onSelectNode: (node: ExecutionNode) => void;
}) {
  const node = findExecutionNodeForWaterfallRow(row);
  const left = clampPercent((row.startMs / timelineEnd) * 100);
  const rawWidth = (row.durationMs / timelineEnd) * 100;
  const width = Math.min(Math.max(rawWidth, 0.8), 100 - left);
  const selected = selectedNodeId === node?.id;
  const color = serviceColor(row.service);

  return (
    <button
      aria-label={`Select row ${row.name}`}
      className={cn(
        "grid w-full min-w-0 grid-cols-[minmax(260px,340px)_minmax(0,1fr)] items-center gap-4 px-3 py-1.5 text-left transition hover:bg-[color-mix(in_srgb,var(--hover)_64%,transparent)] disabled:cursor-default",
        selected && "bg-(--accent-soft) shadow-[inset_2px_0_0_var(--accent)]",
        row.group === "unlinked" && "opacity-82"
      )}
      disabled={!node}
      onClick={() => {
        if (node) {
          onSelectNode(node);
        }
      }}
      type="button"
    >
      <span className="flex min-w-0 items-center gap-1.5 overflow-hidden">
        <span
          className="grid h-7 shrink-0 grid-cols-[1px_minmax(0,1fr)]"
          style={{ marginLeft: row.depth * 16, width: row.depth > 0 ? 18 : 2 }}
        >
          <span className="h-full bg-(--border-subtle)" />
          {row.depth > 0 ? (
            <span className="mt-3 h-px bg-(--border-subtle)" />
          ) : null}
        </span>
        <span
          className="size-2 shrink-0 rounded-xs"
          style={{ backgroundColor: statusColor(row.status) }}
        />
        <span
          className="max-w-28 shrink-0 truncate whitespace-nowrap rounded-xs border px-1.5 py-0.5 font-mono text-[11px] leading-3.5"
          style={{
            backgroundColor: `${color}12`,
            borderColor: `${color}24`,
            color,
          }}
        >
          {row.service}
        </span>
        <span className="max-w-26 shrink-0 truncate font-mono text-[11px] text-(--muted)">
          {row.kind}
        </span>
        {row.fanoutGroupSize ? (
          <span className="shrink-0 rounded-xs px-1.5 py-0.5 font-mono text-[10px] leading-3.5 tint tint-info">
            fan-out {row.fanoutGroupSize}
          </span>
        ) : null}
        {!row.fanoutGroupSize && row.parallelGroupSize ? (
          <span className="shrink-0 rounded-xs px-1.5 py-0.5 font-mono text-[10px] leading-3.5 tint tint-info">
            parallel group
          </span>
        ) : null}
        <span className="truncate font-mono text-[13px] text-(--foreground)">
          {row.name}
        </span>
        <span className="ml-auto font-mono text-xs text-(--muted)">
          {formatRuntimeDuration(row.durationMs)}
        </span>
      </span>
      <span className="relative isolate h-8 min-w-0 overflow-hidden rounded-xs bg-[linear-gradient(90deg,transparent_0%,transparent_24.8%,var(--border-subtle)_25%,transparent_25.2%,transparent_49.8%,var(--border-subtle)_50%,transparent_50.2%,transparent_74.8%,var(--border-subtle)_75%,transparent_75.2%)]">
        <span
          className="absolute top-2 h-4 min-w-0.75 rounded-xs transition-transform"
          style={{
            backgroundColor:
              row.status === "failed" || row.status === "dead"
                ? "#ef4444"
                : color,
            left: `${left}%`,
            opacity: selected ? 1 : 0.82,
            transform: selected ? "scaleY(1.25)" : undefined,
            width: `${width}%`,
          }}
        />
        {row.markers.map((marker) => (
          <TimelineMarker
            key={marker.id}
            marker={marker}
            timelineEnd={timelineEnd}
          />
        ))}
      </span>
    </button>
  );
}

function TimelineMarker({
  marker,
  timelineEnd,
}: {
  marker: WaterfallTimelineMarker;
  timelineEnd: number;
}) {
  const left = clampPercent((marker.startMs / timelineEnd) * 100);

  return (
    <span
      className="absolute top-1 h-6 w-px bg-(--foreground)"
      style={{
        left: `${left}%`,
        opacity:
          marker.status === "failed" || marker.status === "dead" ? 0.9 : 0.46,
      }}
      title={`${marker.kind}: ${marker.name}`}
    />
  );
}

function clampPercent(value: number) {
  return Math.min(100, Math.max(0, value));
}
