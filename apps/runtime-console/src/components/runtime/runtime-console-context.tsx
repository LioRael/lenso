import { useNavigate } from "@tanstack/react-router";
import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  useRef,
  useState,
  type PropsWithChildren,
  type RefObject,
} from "react";

import {
  correlationId,
  functionRuns,
  runtimeEvents,
  runtimeStories,
  timelineItems,
  type RetryTarget,
  type RuntimeRecord,
} from "../../data/mock-runtime";
import { queryDataWithMockFallback } from "../../hooks/runtime-query-data";
import {
  useRuntimeEvents,
  useRuntimeFunctions,
  useRuntimeStories,
} from "../../hooks/use-runtime-queries";
import { isApiMode } from "../../lib/http-client";
import {
  buildRuntimeSearchResults,
  type RuntimeSearchResult,
} from "./runtime-search-model";

export type SearchResult = RuntimeSearchResult;

type RuntimeConsoleContextValue = {
  drawerTarget: RuntimeRecord | null;
  retryTarget: RetryTarget | null;
  commandOpen: boolean;
  activeCorrelationId: string;
  activeStoryTarget: { storyId: string; nodeId?: string } | null;
  searchInputRef: RefObject<HTMLInputElement | null>;
  openDrawer: (target: RuntimeRecord | null) => void;
  closeDrawer: () => void;
  openRetry: (target: RetryTarget | null) => void;
  closeRetry: () => void;
  openCommandPalette: () => void;
  closeCommandPalette: () => void;
  focusGlobalSearch: () => void;
  openTimeline: (nextCorrelationId: string) => void;
  openStory: (storyId: string, nodeId?: string) => void;
  clearStoryTarget: () => void;
  searchRuntime: (query: string) => SearchResult[];
  selectSearchResult: (result: SearchResult) => void;
};

const RuntimeConsoleContext = createContext<RuntimeConsoleContextValue | null>(
  null
);

export function RuntimeConsoleProvider({ children }: PropsWithChildren) {
  const navigate = useNavigate();
  const eventsQuery = useRuntimeEvents();
  const functionsQuery = useRuntimeFunctions();
  const storiesQuery = useRuntimeStories();
  const searchInputRef = useRef<HTMLInputElement>(null);
  const [drawerTarget, setDrawerTarget] = useState<RuntimeRecord | null>(null);
  const [retryTarget, setRetryTarget] = useState<RetryTarget | null>(null);
  const [commandOpen, setCommandOpen] = useState(false);
  const [activeCorrelationId, setActiveCorrelationId] = useState(correlationId);
  const [activeStoryTarget, setActiveStoryTarget] = useState<{
    storyId: string;
    nodeId?: string;
  } | null>(null);

  const openTimeline = useCallback(
    (nextCorrelationId: string) => {
      setActiveCorrelationId(nextCorrelationId);
      void navigate({ to: "/runtime/stories" });
    },
    [navigate]
  );

  const openStory = useCallback(
    (storyId: string, nodeId?: string) => {
      setActiveStoryTarget({ storyId, ...(nodeId ? { nodeId } : {}) });
      void navigate({ to: "/runtime/stories" });
    },
    [navigate]
  );

  const clearStoryTarget = useCallback(() => {
    setActiveStoryTarget(null);
  }, []);

  const searchRuntime = useCallback(
    (query: string) => {
      const normalized = query.trim().toLowerCase();
      if (!normalized) {
        return [];
      }

      return buildRuntimeSearchResults({
        events: queryDataWithMockFallback({
          apiMode: isApiMode(),
          data: eventsQuery.data,
          fallback: runtimeEvents,
          isError: eventsQuery.isError,
        }),
        functions: queryDataWithMockFallback({
          apiMode: isApiMode(),
          data: functionsQuery.data,
          fallback: functionRuns,
          isError: functionsQuery.isError,
        }),
        query: normalized,
        stories: queryDataWithMockFallback({
          apiMode: isApiMode(),
          data: storiesQuery.data,
          fallback: runtimeStories,
          isError: storiesQuery.isError,
        }),
      });
    },
    [
      eventsQuery.data,
      eventsQuery.isError,
      functionsQuery.data,
      functionsQuery.isError,
      storiesQuery.data,
      storiesQuery.isError,
    ]
  );

  const selectSearchResult = useCallback(
    (result: SearchResult) => {
      if (result.kind === "correlation") {
        openTimeline(result.correlationId);
        return;
      }
      if (result.kind === "story") {
        openStory(result.storyId, result.nodeId);
        return;
      }
      setDrawerTarget(result.record);
    },
    [openTimeline, openStory]
  );

  const value = useMemo<RuntimeConsoleContextValue>(
    () => ({
      drawerTarget,
      retryTarget,
      commandOpen,
      activeCorrelationId,
      activeStoryTarget,
      searchInputRef,
      openDrawer: setDrawerTarget,
      closeDrawer: () => setDrawerTarget(null),
      openRetry: setRetryTarget,
      closeRetry: () => setRetryTarget(null),
      openCommandPalette: () => setCommandOpen(true),
      closeCommandPalette: () => setCommandOpen(false),
      focusGlobalSearch: () => searchInputRef.current?.focus(),
      openTimeline,
      openStory,
      clearStoryTarget,
      searchRuntime,
      selectSearchResult,
    }),
    [
      activeCorrelationId,
      activeStoryTarget,
      clearStoryTarget,
      commandOpen,
      drawerTarget,
      openTimeline,
      openStory,
      retryTarget,
      searchRuntime,
      selectSearchResult,
    ]
  );

  return (
    <RuntimeConsoleContext.Provider value={value}>
      {children}
    </RuntimeConsoleContext.Provider>
  );
}

export function useRuntimeConsole() {
  const context = useContext(RuntimeConsoleContext);
  if (!context) {
    throw new Error(
      "useRuntimeConsole must be used within RuntimeConsoleProvider"
    );
  }
  return context;
}

export function resolveTimelineSource(itemId: string): RuntimeRecord | null {
  const event = runtimeEvents.find((item) => item.id === itemId);
  if (event) {
    return { kind: "event", item: event };
  }

  const run = functionRuns.find((item) => item.id === itemId);
  if (run) {
    return { kind: "function", item: run };
  }

  const item = timelineItems.find((timelineItem) => timelineItem.id === itemId);
  return item ? { kind: "timeline", item } : null;
}
