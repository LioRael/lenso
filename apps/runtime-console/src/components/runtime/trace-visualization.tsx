import type { TraceRun, TraceSpan } from "../../data/mock-runtime";
import { FlameView } from "./flame-view";
import { FlowView } from "./flow-view";
import { HeatmapPlaceholderView } from "./heatmap-placeholder-view";
import { RuntimeStoryView } from "./runtime-story-view";
import { StoryTimelineView } from "./story-timeline-view";
import type { TraceViewMode } from "./trace-tabs";
import { TraceTabs } from "./trace-tabs";
import { WaterfallView } from "./waterfall-view";

export function TraceVisualization({
  mode,
  selectedSpanId,
  setMode,
  trace,
  onRetrySpan,
  onSelectSpan,
}: {
  trace: TraceRun;
  mode: TraceViewMode;
  selectedSpanId: string | null;
  setMode: (mode: TraceViewMode) => void;
  onSelectSpan: (span: TraceSpan) => void;
  onRetrySpan: (span: TraceSpan) => void;
}) {
  return (
    <section className="isolate grid h-full min-h-0 min-w-0 grid-rows-[32px_minmax(0,1fr)] overflow-hidden">
      <TraceTabs mode={mode} onChange={setMode} />
      <div className="min-h-0 min-w-0 overflow-hidden">
        {mode === "story" ? (
          <RuntimeStoryView
            onRetryNode={(node) => onRetrySpan(node.span)}
            onSelectSpan={onSelectSpan}
            selectedSpanId={selectedSpanId}
            trace={trace}
          />
        ) : null}
        {mode === "graph" ? (
          <FlowView
            onSelectSpan={onSelectSpan}
            selectedSpanId={selectedSpanId}
            trace={trace}
          />
        ) : null}
        {mode === "timeline" ? (
          <StoryTimelineView
            onSelectSpan={onSelectSpan}
            selectedSpanId={selectedSpanId}
            trace={trace}
          />
        ) : null}
        {mode === "waterfall" ? (
          <WaterfallView
            onSelectSpan={onSelectSpan}
            selectedSpanId={selectedSpanId}
            trace={trace}
          />
        ) : null}
        {mode === "flame" ? (
          <FlameView
            onSelectSpan={onSelectSpan}
            selectedSpanId={selectedSpanId}
            trace={trace}
          />
        ) : null}
        {mode === "heatmap" ? <HeatmapPlaceholderView trace={trace} /> : null}
      </div>
    </section>
  );
}
