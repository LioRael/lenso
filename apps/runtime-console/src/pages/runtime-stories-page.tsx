import { useGSAP } from "@gsap/react";
import gsap from "gsap";
import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
} from "react";

import { ExecutionInspector } from "../components/runtime/execution-inspector";
import { ResizeHandle } from "../components/runtime/resize-handle";
import { useRuntimeConsole } from "../components/runtime/runtime-console-context";
import { RuntimeStoryVisualization } from "../components/runtime/runtime-story-visualization";
import { ServiceSummaryStrip } from "../components/runtime/service-summary-strip";
import { StoryHeader } from "../components/runtime/story-header";
import { StoryList } from "../components/runtime/story-list";
import type { StoryViewMode } from "../components/runtime/story-tabs";
import { EmptyState } from "../components/ui/empty-state";
import {
  isRetryable,
  type RuntimeStory,
  type ExecutionNode,
} from "../data/mock-runtime";
import { useListKeyboard } from "../hooks/use-list-keyboard";
import { usePersistedLayout } from "../hooks/use-persisted-layout";
import { useRuntimeStories } from "../hooks/use-runtime-queries";
import {
  resizeServicesPanelLayout,
  resizeExecutionInspectorLayout,
  resizeStoryListWidth,
  runtimeStoriesLayoutDefaults,
} from "./runtime-stories-layout";

gsap.registerPlugin(useGSAP);

type InspectorTab =
  | "info"
  | "attributes"
  | "events"
  | "errors"
  | "logs"
  | "context";

const emptyStories: RuntimeStory[] = [];
export const runtimeStoriesDefaultViewMode = "story" satisfies StoryViewMode;

export function RuntimeStoriesPage() {
  const { activeStoryTarget, clearStoryTarget, openRetry } =
    useRuntimeConsole();
  const storiesQuery = useRuntimeStories();
  const stories = storiesQuery.data ?? emptyStories;
  const [query, setQuery] = useState("");
  const [selectedStoryId, setSelectedStoryId] = useState<string | null>(null);
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);
  const [displayedNode, setDisplayedNode] = useState<ExecutionNode | null>(
    null
  );
  const [storyDetailClosed, setStoryDetailClosed] = useState(false);
  const [servicesExpanded, setServicesExpanded] = useState(true);
  const [mode, setMode] = useState<StoryViewMode>(
    runtimeStoriesDefaultViewMode
  );
  const [inspectorTab, setInspectorTab] = useState<InspectorTab>("info");
  const workbenchRef = useRef<HTMLDivElement | null>(null);
  const inspectorPanelRef = useRef<HTMLDivElement | null>(null);
  const previousInspectorOpenRef = useRef(false);
  const [layout, setLayout, resetLayout] = usePersistedLayout(
    "runtime-console:stories-layout",
    runtimeStoriesLayoutDefaults
  );
  const storiesLayout = { ...runtimeStoriesLayoutDefaults, ...layout };
  const inspectorWidthRef = useRef(storiesLayout.inspectorWidth);
  const servicesExpandedRef = useRef(servicesExpanded);
  const servicesHeightRef = useRef(storiesLayout.servicesHeight);

  useEffect(() => {
    inspectorWidthRef.current = storiesLayout.inspectorWidth;
  }, [storiesLayout.inspectorWidth]);

  useEffect(() => {
    servicesExpandedRef.current = servicesExpanded;
  }, [servicesExpanded]);

  useEffect(() => {
    servicesHeightRef.current = storiesLayout.servicesHeight;
  }, [storiesLayout.servicesHeight]);

  const visibleStories = useMemo(() => {
    const normalized = query.trim().toLowerCase();
    return stories.filter((story) => {
      if (!normalized) {
        return true;
      }
      return [
        story.id,
        story.name,
        story.service,
        story.source,
        story.correlationId,
      ].some((value) => value.toLowerCase().includes(normalized));
    });
  }, [query, stories]);

  const targetStory = activeStoryTarget
    ? stories.find((story) => story.id === activeStoryTarget.storyId)
    : null;
  const selectedStory =
    storyDetailClosed && !targetStory
      ? null
      : (targetStory ??
        stories.find((story) => story.id === selectedStoryId) ??
        visibleStories[0] ??
        null);
  const selectedNode =
    selectedStory?.nodes.find((node) => {
      const targetNodeId = activeStoryTarget?.nodeId ?? selectedNodeId;
      return targetNodeId ? node.id === targetNodeId : false;
    }) ?? null;
  const selectedStoryIndex = Math.max(
    0,
    visibleStories.findIndex((story) => story.id === selectedStory?.id)
  );
  const inspectorOpen = selectedNode !== null;
  const hasInspector = displayedNode !== null;
  const listColumn = `clamp(220px,24vw,${storiesLayout.listWidth}px)`;
  const inspectorColumn = `clamp(280px,30vw,${storiesLayout.inspectorWidth}px)`;
  const gridTemplateColumns = hasInspector
    ? `${listColumn} 1px minmax(0,1fr) calc(1px * var(--story-inspector-open)) minmax(0,calc(${inspectorColumn} * var(--story-inspector-open)))`
    : `${listColumn} 1px minmax(0,1fr)`;
  const showServicesPanel = mode === "waterfall" || mode === "flame";

  useEffect(() => {
    if (selectedNode) {
      setDisplayedNode(selectedNode);
    }
  }, [selectedNode]);

  useGSAP(
    () => {
      const workbench = workbenchRef.current;
      const inspectorPanel = inspectorPanelRef.current;

      if (!workbench || (!displayedNode && !previousInspectorOpenRef.current)) {
        return;
      }

      const reduceMotion = window.matchMedia(
        "(prefers-reduced-motion: reduce)"
      ).matches;
      const nextOpen = inspectorOpen ? 1 : 0;
      const hasOpenStateChanged =
        previousInspectorOpenRef.current !== inspectorOpen;
      previousInspectorOpenRef.current = inspectorOpen;
      gsap.killTweensOf(workbench);
      gsap.killTweensOf(inspectorPanel);

      if (!hasOpenStateChanged) {
        gsap.set(workbench, {
          "--story-inspector-open": nextOpen,
        });
        gsap.set(inspectorPanel, {
          autoAlpha: nextOpen,
          x: inspectorOpen ? 0 : 18,
        });
        return;
      }

      if (reduceMotion) {
        gsap.set(workbench, {
          "--story-inspector-open": nextOpen,
        });
        gsap.set(inspectorPanel, {
          autoAlpha: nextOpen,
          x: 0,
        });
        if (!inspectorOpen) {
          setDisplayedNode(null);
        }
        return;
      }

      gsap.to(workbench, {
        "--story-inspector-open": nextOpen,
        duration: inspectorOpen ? 0.32 : 0.24,
        ease: inspectorOpen ? "power3.out" : "power2.inOut",
        onComplete: () => {
          if (!inspectorOpen) {
            setDisplayedNode(null);
          }
        },
      });
      gsap.fromTo(
        inspectorPanel,
        {
          autoAlpha: inspectorOpen ? 0 : 1,
          x: inspectorOpen ? 24 : 0,
        },
        {
          autoAlpha: inspectorOpen ? 1 : 0,
          duration: inspectorOpen ? 0.24 : 0.16,
          ease: inspectorOpen ? "power2.out" : "power2.in",
          x: inspectorOpen ? 0 : 18,
        }
      );
    },
    {
      dependencies: [
        displayedNode?.id ?? null,
        inspectorOpen,
        storiesLayout.inspectorWidth,
        storiesLayout.servicesHeight,
      ],
      scope: workbenchRef,
    }
  );

  const selectStory = (story: RuntimeStory) => {
    setStoryDetailClosed(false);
    clearStoryTarget();
    setSelectedStoryId(story.id);
    setSelectedNodeId(null);
    setInspectorTab("info");
  };

  const closeStoryDetail = () => {
    setStoryDetailClosed(true);
    clearStoryTarget();
    setSelectedStoryId(null);
    setSelectedNodeId(null);
    setDisplayedNode(null);
    setInspectorTab("info");
  };

  const resizeStoryList = (deltaX: number) => {
    setLayout((current) => ({
      ...current,
      listWidth: resizeStoryListWidth(current.listWidth, deltaX),
    }));
  };

  const resizeInspector = (deltaX: number) => {
    const next = resizeExecutionInspectorLayout({
      currentWidth: inspectorWidthRef.current,
      deltaX,
    });
    inspectorWidthRef.current = next.width;
    setLayout((current) => ({
      ...current,
      inspectorWidth: next.width,
    }));
    if (!next.open) {
      clearStoryTarget();
      setSelectedNodeId(null);
      setInspectorTab("info");
    }
  };

  const resizeServices = (deltaY: number) => {
    const next = resizeServicesPanelLayout({
      currentHeight: servicesHeightRef.current,
      deltaY,
      expanded: servicesExpandedRef.current,
    });
    servicesExpandedRef.current = next.expanded;
    servicesHeightRef.current = next.height;
    setServicesExpanded(next.expanded);
    setLayout((current) => ({
      ...current,
      servicesHeight: next.height,
    }));
  };

  const selectNode = (node: ExecutionNode) => {
    const ownerStory = stories.find((story) =>
      story.nodes.some((item) => item.id === node.id)
    );
    setStoryDetailClosed(false);
    setSelectedStoryId(ownerStory?.id ?? selectedStory?.id ?? selectedStoryId);
    clearStoryTarget();
    setSelectedNodeId(node.id);
    setInspectorTab(
      node.status === "failed" || node.status === "dead" ? "errors" : "info"
    );
  };

  const retryNode = (node: ExecutionNode) => {
    selectNode(node);
    if (isRetryable(node.status) && node.retryable) {
      openRetry({
        attempts: node.attempts ?? 1,
        id: node.id,
        kind: "timeline",
        maxAttempts: node.maxAttempts ?? 3,
        name: node.name,
        status: node.status,
      });
    }
  };

  useListKeyboard({
    items: visibleStories,
    onOpen: selectStory,
    onRetry: (story) => {
      const retryableNode = story.nodes.find(
        (node) => isRetryable(node.status) && node.retryable
      );
      if (retryableNode) {
        selectStory(story);
        selectNode(retryableNode);
        openRetry({
          attempts: retryableNode.attempts ?? 1,
          id: retryableNode.id,
          kind: "timeline",
          maxAttempts: retryableNode.maxAttempts ?? 3,
          name: retryableNode.name,
          status: retryableNode.status,
        });
      }
    },
    selectedIndex: selectedStoryIndex,
    setSelectedIndex: (index) => {
      const story = visibleStories[index];
      if (story) {
        selectStory(story);
      }
    },
  });

  if (storiesQuery.isLoading) {
    return (
      <div className="font-mono text-xs text-slate-500">loading stories...</div>
    );
  }

  if (storiesQuery.isError) {
    return (
      <div className="font-mono text-xs text-rose-300">
        story workbench unavailable
      </div>
    );
  }

  return (
    <div className="h-full overflow-hidden bg-(--background) text-(--foreground)">
      <div
        ref={workbenchRef}
        className="grid h-full min-w-0 overflow-hidden"
        style={
          {
            "--story-inspector-open": previousInspectorOpenRef.current ? 1 : 0,
            gridTemplateColumns,
          } as CSSProperties
        }
      >
        <StoryList
          onSelect={selectStory}
          query={query}
          selectedStoryId={selectedStory?.id ?? null}
          setQuery={setQuery}
          stories={visibleStories}
        />

        <ResizeHandle
          ariaLabel="Resize story list panel"
          onReset={resetLayout}
          onResize={resizeStoryList}
        />

        <main
          className="grid min-h-0 min-w-0 overflow-hidden"
          style={{
            gridTemplateRows: selectedStory
              ? showServicesPanel
                ? "auto minmax(0,1fr) auto auto"
                : "auto minmax(0,1fr)"
              : "minmax(0,1fr)",
          }}
        >
          {selectedStory ? (
            <>
              <StoryHeader
                onClose={closeStoryDetail}
                onSelectNode={selectNode}
                story={selectedStory}
              />

              <RuntimeStoryVisualization
                mode={mode}
                onRetryNode={retryNode}
                onSelectNode={selectNode}
                selectedNodeId={selectedNode?.id ?? null}
                setMode={setMode}
                story={selectedStory}
              />

              {showServicesPanel ? (
                <>
                  <ResizeHandle
                    ariaLabel="Resize services panel"
                    axis="vertical"
                    onReset={resetLayout}
                    onResize={resizeServices}
                  />

                  <ServiceSummaryStrip
                    expanded={servicesExpanded}
                    height={storiesLayout.servicesHeight}
                    onExpandedChange={setServicesExpanded}
                    story={selectedStory}
                  />
                </>
              ) : null}
            </>
          ) : (
            <EmptyState className="h-full bg-(--surface)">
              <EmptyState.Title>No story selected</EmptyState.Title>
            </EmptyState>
          )}
        </main>

        {selectedStory && displayedNode ? (
          <>
            <ResizeHandle
              ariaLabel="Resize story inspector panel"
              onReset={resetLayout}
              onResize={resizeInspector}
            />

            <div
              ref={inspectorPanelRef}
              className="relative z-0 min-h-0 min-w-0 overflow-hidden"
              style={{
                pointerEvents: inspectorOpen ? "auto" : "none",
              }}
            >
              <ExecutionInspector
                activeTab={inspectorTab}
                onClearSelection={() => {
                  setSelectedStoryId(selectedStory.id);
                  clearStoryTarget();
                  setSelectedNodeId(null);
                  setInspectorTab("info");
                }}
                selectedNode={displayedNode}
                setActiveTab={setInspectorTab}
                story={selectedStory}
              />
            </div>
          </>
        ) : null}
      </div>
    </div>
  );
}
