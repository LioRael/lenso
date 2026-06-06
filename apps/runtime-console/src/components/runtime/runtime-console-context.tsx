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
  functionRuns,
  runtimeEvents,
  runtimeStories,
  type RetryTarget,
} from "../../data/mock-runtime";
import { queryDataWithMockFallback } from "../../hooks/runtime-query-data";
import {
  useRuntimeEvents,
  useRuntimeFunctions,
  useRuntimeStories,
} from "../../hooks/use-runtime-queries";
import { isApiMode } from "../../lib/http-client";
import { runtimeStoriesPath } from "../../modules/story-console";
import { adminActionsPath } from "../../pages/admin-actions-model";
import { functionsPath } from "../../pages/operations-url-model";
import { remoteProxyCallsPath } from "../../pages/remote-proxy-calls-model";
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
  retryTarget: RetryTarget | null;
  commandOpen: boolean;
  activeStoryTarget: { storyId: string; nodeId?: string } | null;
  searchInputRef: RefObject<HTMLInputElement | null>;
  openRetry: (target: RetryTarget | null) => void;
  closeRetry: () => void;
  openCommandPalette: () => void;
  closeCommandPalette: () => void;
  focusGlobalSearch: () => void;
  openTimeline: (nextCorrelationId: string) => void;
  openStory: (storyId: string, nodeId?: string) => void;
  openStoryTarget: (target: RuntimeStoryTargetInput) => void;
  openRemoteCalls: (correlationId?: string, selectedId?: string) => void;
  openAdminActions: (correlationId?: string, selectedId?: string) => void;
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
  const [retryTarget, setRetryTarget] = useState<RetryTarget | null>(null);
  const [commandOpen, setCommandOpen] = useState(false);
  const [activeStoryTarget, setActiveStoryTarget] = useState<{
    storyId: string;
    nodeId?: string;
  } | null>(null);

  const openTimeline = useCallback(
    (nextCorrelationId: string) => {
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

  const openAdminActions = useCallback(
    (nextCorrelationId?: string, selectedId?: string) => {
      const filters = nextCorrelationId
        ? {
            correlationId: nextCorrelationId,
            ...(selectedId ? { selectedId } : {}),
          }
        : {};
      void navigate({
        to: adminActionsPath(filters),
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
      if (result.kind === "event") {
        openStoryTarget({
          correlationId: result.correlationId,
          nodeIdCandidates: [result.id],
        });
        return;
      }
      if (result.kind === "function") {
        void navigate({
          to: functionsPath({ selectedId: result.id }),
        });
      }
    },
    [navigate, openTimeline, openStory, openStoryTarget]
  );

  const value = useMemo<RuntimeConsoleContextValue>(
    () => ({
      retryTarget,
      commandOpen,
      activeStoryTarget,
      searchInputRef,
      openRetry: setRetryTarget,
      closeRetry: () => setRetryTarget(null),
      openCommandPalette: () => setCommandOpen(true),
      closeCommandPalette: () => setCommandOpen(false),
      focusGlobalSearch: () => searchInputRef.current?.focus(),
      openTimeline,
      openStory,
      openStoryTarget,
      openRemoteCalls,
      openAdminActions,
      clearStoryTarget,
      searchRuntime,
      selectSearchResult,
    }),
    [
      activeStoryTarget,
      clearStoryTarget,
      commandOpen,
      openTimeline,
      openAdminActions,
      openRemoteCalls,
      openStory,
      openStoryTarget,
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
