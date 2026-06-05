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
  type TimelineItem,
} from "../../data/mock-runtime";
import { queryDataWithMockFallback } from "../../hooks/runtime-query-data";
import {
  useRuntimeEvents,
  useRuntimeFunctions,
  useRuntimeStories,
} from "../../hooks/use-runtime-queries";
import { isApiMode } from "../../lib/http-client";
import { remoteProxyCallsPath } from "../../pages/remote-proxy-calls-model";
import { runtimeStoriesPath } from "../../pages/runtime-stories-url-model";
import {
  buildRuntimeSearchResults,
  type RuntimeSearchResult,
} from "./runtime-search-model";
import {
  resolveRuntimeStoryTarget,
  type RuntimeStoryTargetInput,
} from "./runtime-story-target";

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
  openStoryTarget: (target: RuntimeStoryTargetInput) => void;
  openRemoteCalls: (correlationId?: string, selectedId?: string) => void;
  openTimelineSource: (item: TimelineItem) => void;
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
      setActiveStoryTarget({ storyId: nextCorrelationId });
      void navigate({
        to: runtimeStoriesPath({ storyId: nextCorrelationId }),
      });
    },
    [navigate]
  );

  const openStory = useCallback(
    (storyId: string, nodeId?: string) => {
      setActiveStoryTarget({ storyId, ...(nodeId ? { nodeId } : {}) });
      void navigate({
        to: runtimeStoriesPath({
          ...(nodeId ? { nodeId } : {}),
          storyId,
        }),
      });
    },
    [navigate]
  );

  const openRemoteCalls = useCallback(
    (nextCorrelationId?: string, selectedId?: string) => {
      const filters = nextCorrelationId
        ? {
            correlationId: nextCorrelationId,
            ...(selectedId ? { selectedId } : {}),
          }
        : {};
      void navigate({
        to: remoteProxyCallsPath(filters),
      });
    },
    [navigate]
  );

  const clearStoryTarget = useCallback(() => {
    setActiveStoryTarget(null);
  }, []);

  const resolvedEvents = useMemo(
    () =>
      queryDataWithMockFallback({
        apiMode: isApiMode(),
        data: eventsQuery.data,
        fallback: runtimeEvents,
        isError: eventsQuery.isError,
      }),
    [eventsQuery.data, eventsQuery.isError]
  );

  const resolvedFunctions = useMemo(
    () =>
      queryDataWithMockFallback({
        apiMode: isApiMode(),
        data: functionsQuery.data,
        fallback: functionRuns,
        isError: functionsQuery.isError,
      }),
    [functionsQuery.data, functionsQuery.isError]
  );

  const resolvedStories = useMemo(
    () =>
      queryDataWithMockFallback({
        apiMode: isApiMode(),
        data: storiesQuery.data,
        fallback: runtimeStories,
        isError: storiesQuery.isError,
      }),
    [storiesQuery.data, storiesQuery.isError]
  );

  const openStoryTarget = useCallback(
    (target: RuntimeStoryTargetInput) => {
      const resolvedTarget = resolveRuntimeStoryTarget(resolvedStories, target);
      setActiveStoryTarget(resolvedTarget);
      void navigate({
        to: runtimeStoriesPath({
          ...(resolvedTarget.nodeId ? { nodeId: resolvedTarget.nodeId } : {}),
          storyId: resolvedTarget.storyId,
        }),
      });
    },
    [navigate, resolvedStories]
  );

  const openTimelineSource = useCallback(
    (item: TimelineItem) => {
      if (item.type === "remote_proxy_call") {
        openRemoteCalls(item.correlationId, remoteProxyCallSelectedId(item));
        return;
      }

      const record = resolveTimelineSourceRecord(item, {
        events: resolvedEvents,
        functions: resolvedFunctions,
      });
      setDrawerTarget(record ?? { kind: "timeline", item });
    },
    [openRemoteCalls, resolvedEvents, resolvedFunctions]
  );

  const searchRuntime = useCallback(
    (query: string) => {
      const normalized = query.trim().toLowerCase();
      if (!normalized) {
        return [];
      }

      return buildRuntimeSearchResults({
        events: resolvedEvents,
        functions: resolvedFunctions,
        query: normalized,
        stories: resolvedStories,
      });
    },
    [resolvedEvents, resolvedFunctions, resolvedStories]
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
      openStoryTarget,
      openRemoteCalls,
      openTimelineSource,
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
      openRemoteCalls,
      openStory,
      openStoryTarget,
      openTimelineSource,
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

export function resolveTimelineSourceRecord(
  item: TimelineItem,
  sources: {
    events: typeof runtimeEvents;
    functions: typeof functionRuns;
  }
): RuntimeRecord | null {
  const sourceId = item.detailId ?? item.id;

  if (item.type === "outbox_event" || item.type === "event") {
    const event = sources.events.find((candidate) => candidate.id === sourceId);
    return event ? { kind: "event", item: event } : null;
  }

  if (item.type === "function_run" || item.type === "function") {
    const run = sources.functions.find(
      (candidate) => candidate.id === sourceId
    );
    return run ? { kind: "function", item: run } : null;
  }

  return resolveTimelineSource(sourceId);
}

export function remoteProxyCallSelectedId(item: TimelineItem) {
  const sourceId = item.detailId ?? item.id;
  return sourceId.startsWith("remoteproxy_")
    ? sourceId.slice("remoteproxy_".length)
    : sourceId;
}
