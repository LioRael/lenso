import type { TraceRun, TraceSpan } from "../../data/mock-runtime";
import { FlameView } from "./flame-view";
import { FlowView } from "./flow-view";
import { HeatmapView } from "./heatmap-view";
import { StoryTimelineView } from "./story-timeline-view";
import type { TraceViewMode } from "./trace-tabs";
import { TraceTabs } from "./trace-tabs";
import { WaterfallView } from "./waterfall-view";

export function TraceVisualization({
  mode,
  selectedSpanId,
  setMode,
  trace,
  onSelectSpan,
}: {
  trace: TraceRun;
  mode: TraceViewMode;
  selectedSpanId: string | null;
  setMode: (mode: TraceViewMode) => void;
  onSelectSpan: (span: TraceSpan) => void;
}) {
  return (
    <section className="isolate grid h-full min-h-0 min-w-0 grid-rows-[32px_minmax(0,1fr)] overflow-hidden">
      <TraceTabs mode={mode} onChange={setMode} />
      <div className="min-h-0 min-w-0 overflow-hidden">
        {mode === "story" ? (
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
        {mode === "heatmap" ? (
          <HeatmapView
            onSelectSpan={onSelectSpan}
            selectedSpanId={selectedSpanId}
            trace={trace}
          />
        ) : null}
        {mode === "flow" ? (
          <FlowView
            onSelectSpan={onSelectSpan}
            selectedSpanId={selectedSpanId}
            trace={trace}
          />
        ) : null}
      </div>
    </section>
  );
}
