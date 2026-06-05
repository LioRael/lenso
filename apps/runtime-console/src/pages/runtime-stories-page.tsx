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
import {
  defaultExecutionInspectorTab,
  type ExecutionInspectorTab,
} from "../components/runtime/execution-inspector-model";
import { ResizeHandle } from "../components/runtime/resize-handle";
import { useRuntimeConsole } from "../components/runtime/runtime-console-context";
import { findStoryByCorrelation } from "../components/runtime/runtime-story-target";
import { RuntimeStoryVisualization } from "../components/runtime/runtime-story-visualization";
import { ServiceSummaryStrip } from "../components/runtime/service-summary-strip";
import { StoryHeader } from "../components/runtime/story-header";
import { StoryList } from "../components/runtime/story-list";
import type { StoryViewMode } from "../components/runtime/story-tabs";
import { EmptyState } from "../components/ui/empty-state";
import {
  retryTargetForNode,
  type RuntimeStory,
  type ExecutionNode,
} from "../data/mock-runtime";
import { useListKeyboard } from "../hooks/use-list-keyboard";
import { usePersistedLayout } from "../hooks/use-persisted-layout";
import { useRuntimeStories } from "../hooks/use-runtime-queries";
import { shouldCloseInspectorOnEscape } from "./runtime-stories-keyboard";
import {
  resizeServicesPanelLayout,
  resizeExecutionInspectorLayout,
  resizeStoryListWidth,
  runtimeStoriesLayoutDefaults,
} from "./runtime-stories-layout";
import { resolveSelectedRuntimeStory } from "./runtime-stories-selection";
import {
  readExecutionInspectorTab,
  readRuntimeStoriesParam,
  readStoryViewMode,
  replaceRuntimeStoriesUrl,
  runtimeStoriesPath,
  storyUrlId,
} from "./runtime-stories-url-model";

gsap.registerPlugin(useGSAP);

const emptyStories: RuntimeStory[] = [];
const selectedStoryStorageKey = "runtime-console:selected-story-correlation-id";
export const runtimeStoriesDefaultViewMode = "story" satisfies StoryViewMode;

export function RuntimeStoriesPage() {
  const { activeStoryTarget, clearStoryTarget, openRetry } =
    useRuntimeConsole();
  const storiesQuery = useRuntimeStories();
  const stories = storiesQuery.data ?? emptyStories;
  const [query, setQuery] = useState(() => readRuntimeStoriesParam("q"));
  const [selectedStoryId, setSelectedStoryId] = useState<string | null>(
    () =>
      readRuntimeStoriesParam("story") ||
      (typeof window === "undefined"
        ? null
        : window.localStorage.getItem(selectedStoryStorageKey))
  );
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(
    () => readRuntimeStoriesParam("node") || null
  );
  const [displayedNode, setDisplayedNode] = useState<ExecutionNode | null>(
    null
  );
  const [storyDetailClosed, setStoryDetailClosed] = useState(false);
  const [servicesExpanded, setServicesExpanded] = useState(true);
  const [mode, setMode] = useState<StoryViewMode>(() =>
    readStoryViewMode(readRuntimeStoriesParam("view"))
  );
  const [inspectorTab, setInspectorTab] = useState<ExecutionInspectorTab>(() =>
    readExecutionInspectorTab(readRuntimeStoriesParam("tab"))
  );
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
    ? findStoryByCorrelation(stories, activeStoryTarget.storyId)
    : null;
  const selectedStory =
    targetStory ??
    resolveSelectedRuntimeStory(
      visibleStories,
      selectedStoryId,
      storyDetailClosed
    );
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
    if (storiesQuery.isLoading) {
      return;
    }
    replaceRuntimeStoriesUrl(
      runtimeStoriesPath({
        inspectorTab: selectedNode ? inspectorTab : "overview",
        nodeId: selectedNode?.id ?? null,
        query,
        storyId: storyUrlId(selectedStory),
        viewMode: mode,
      })
    );
  }, [
    inspectorTab,
    mode,
    query,
    selectedNode,
    selectedStory,
    storiesQuery.isLoading,
  ]);

  useEffect(() => {
    if (selectedNode) {
      setDisplayedNode(selectedNode);
    }
  }, [selectedNode]);

  useEffect(() => {
    if (!inspectorOpen) {
      return;
    }

    const onKeyDown = (event: KeyboardEvent) => {
      if (!shouldCloseInspectorOnEscape(event)) {
        return;
      }

      event.preventDefault();
      clearStoryTarget();
      setSelectedNodeId(null);
      setInspectorTab("overview");
    };

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [clearStoryTarget, inspectorOpen]);

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
    setSelectedStoryId(story.correlationId);
    window.localStorage.setItem(selectedStoryStorageKey, story.correlationId);
    setSelectedNodeId(null);
    setInspectorTab("overview");
  };

  const closeStoryDetail = () => {
    setStoryDetailClosed(true);
    clearStoryTarget();
    setSelectedStoryId(null);
    window.localStorage.removeItem(selectedStoryStorageKey);
    setSelectedNodeId(null);
    setDisplayedNode(null);
    setInspectorTab("overview");
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
      setInspectorTab("overview");
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
    const nextStoryId =
      ownerStory?.correlationId ??
      selectedStory?.correlationId ??
      selectedStoryId;
    setSelectedStoryId(nextStoryId);
    if (nextStoryId) {
      window.localStorage.setItem(selectedStoryStorageKey, nextStoryId);
    }
    clearStoryTarget();
    setSelectedNodeId(node.id);
    setInspectorTab(defaultExecutionInspectorTab(node));
  };

  const retryNode = (node: ExecutionNode) => {
    selectNode(node);
    const retryTarget = retryTargetForNode(node);
    if (retryTarget) {
      openRetry(retryTarget);
    }
  };

  useListKeyboard({
    items: visibleStories,
    onOpen: selectStory,
    onRetry: (story) => {
      const retryableNode = story.nodes.find(
        (node) => retryTargetForNode(node) !== null
      );
      if (retryableNode) {
        selectStory(story);
        selectNode(retryableNode);
        const retryTarget = retryTargetForNode(retryableNode);
        if (retryTarget) {
          openRetry(retryTarget);
        }
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
      <div className="grid h-full grid-cols-[clamp(220px,24vw,320px)_1px_minmax(0,1fr)] overflow-hidden bg-(--background)">
        <StoryListSkeleton />
        <div className="bg-(--border-subtle)" />
        <EmptyState className="h-full bg-(--surface)">
          <EmptyState.Title>Loading stories</EmptyState.Title>
          <EmptyState.Description>
            Runtime executions are being loaded from the selected data source.
          </EmptyState.Description>
        </EmptyState>
      </div>
    );
  }

  if (storiesQuery.isError) {
    return (
      <EmptyState className="h-full bg-(--surface)">
        <EmptyState.Title>Story Explorer unavailable</EmptyState.Title>
        <EmptyState.Description>
          {storiesQuery.error instanceof Error
            ? storiesQuery.error.message
            : "Runtime story data could not be loaded."}
        </EmptyState.Description>
      </EmptyState>
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
              <EmptyState.Title>
                {stories.length === 0
                  ? "No runtime stories"
                  : query
                    ? "No matching stories"
                    : "No story selected"}
              </EmptyState.Title>
              <EmptyState.Description>
                {stories.length === 0
                  ? "The backend returned an empty runtime story list."
                  : query
                    ? "Try a different story, service, event, function, or correlation filter."
                    : "Select a story from the explorer to inspect its execution."}
              </EmptyState.Description>
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
                  setSelectedStoryId(storyUrlId(selectedStory));
                  clearStoryTarget();
                  setSelectedNodeId(null);
                  setInspectorTab("overview");
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

function StoryListSkeleton() {
  return (
    <aside className="grid h-full min-h-0 min-w-0 grid-rows-[auto_auto_auto_minmax(0,1fr)] overflow-hidden bg-(--background)">
      <div className="border-b border-(--border-subtle) bg-(--surface) px-3 py-2">
        <div className="h-4 w-20 bg-(--elevated)" />
        <div className="mt-1 h-3 w-28 bg-(--elevated)" />
      </div>
      <div className="h-8 border-b border-(--border-subtle) px-3 py-2">
        <div className="h-3 w-full bg-(--elevated)" />
      </div>
      <div className="h-6 border-b border-(--border-subtle) px-3 py-2">
        <div className="h-2 w-24 bg-(--elevated)" />
      </div>
      <div className="grid content-start gap-0">
        {Array.from({ length: 6 }, (_, index) => (
          <div className="border-b border-(--border-subtle) p-3" key={index}>
            <div className="h-3 w-3/4 bg-(--elevated)" />
            <div className="mt-2 h-2 w-5/6 bg-(--elevated)" />
            <div className="mt-3 flex gap-1.5">
              <span className="h-3 w-12 bg-(--elevated)" />
              <span className="h-3 w-14 bg-(--elevated)" />
              <span className="h-3 w-10 bg-(--elevated)" />
            </div>
          </div>
        ))}
      </div>
    </aside>
  );
}
