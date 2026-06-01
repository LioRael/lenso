import type { RuntimeStory, ExecutionNode } from "../../data/mock-runtime";
import { FlameView } from "./flame-view";
import { FlowView } from "./flow-view";
import { HeatmapView } from "./heatmap-view";
import { RuntimeStoryView } from "./runtime-story-view";
import type { StoryViewMode } from "./story-tabs";
import { StoryTabs } from "./story-tabs";
import { StoryTimelineView } from "./story-timeline-view";
import { WaterfallView } from "./waterfall-view";

export function RuntimeStoryVisualization({
  mode,
  selectedNodeId,
  setMode,
  story,
  onRetryNode,
  onSelectNode,
}: {
  story: RuntimeStory;
  mode: StoryViewMode;
  selectedNodeId: string | null;
  setMode: (mode: StoryViewMode) => void;
  onSelectNode: (node: ExecutionNode) => void;
  onRetryNode: (node: ExecutionNode) => void;
}) {
  return (
    <section className="isolate grid h-full min-h-0 min-w-0 grid-rows-[32px_minmax(0,1fr)] overflow-hidden">
      <StoryTabs mode={mode} onChange={setMode} />
      <div className="min-h-0 min-w-0 overflow-hidden">
        {mode === "story" ? (
          <RuntimeStoryView
            onRetryNode={(node) => onRetryNode(node.node)}
            onSelectNode={onSelectNode}
            selectedNodeId={selectedNodeId}
            story={story}
          />
        ) : null}
        {mode === "graph" ? (
          <FlowView
            onSelectNode={onSelectNode}
            selectedNodeId={selectedNodeId}
            story={story}
          />
        ) : null}
        {mode === "timeline" ? (
          <StoryTimelineView
            onSelectNode={onSelectNode}
            selectedNodeId={selectedNodeId}
            story={story}
          />
        ) : null}
        {mode === "waterfall" ? (
          <WaterfallView
            onSelectNode={onSelectNode}
            selectedNodeId={selectedNodeId}
            story={story}
          />
        ) : null}
        {mode === "flame" ? (
          <FlameView
            onSelectNode={onSelectNode}
            selectedNodeId={selectedNodeId}
            story={story}
          />
        ) : null}
        {mode === "heatmap" ? (
          <HeatmapView
            onSelectNode={onSelectNode}
            selectedNodeId={selectedNodeId}
            story={story}
          />
        ) : null}
      </div>
    </section>
  );
}
