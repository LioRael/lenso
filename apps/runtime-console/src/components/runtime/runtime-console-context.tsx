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
  timelineItems,
  runtimeStories,
  type RetryTarget,
  type RuntimeRecord,
} from "../../data/mock-runtime";
import {
  useRuntimeEvents,
  useRuntimeFunctions,
} from "../../hooks/use-runtime-queries";

export type SearchResult =
  | {
      kind: "event";
      id: string;
      title: string;
      subtitle: string;
      correlationId: string;
      record: RuntimeRecord;
    }
  | {
      kind: "function";
      id: string;
      title: string;
      subtitle: string;
      correlationId: string;
      record: RuntimeRecord;
    }
  | {
      kind: "story";
      id: string;
      title: string;
      subtitle: string;
      correlationId: string;
      storyId: string;
      nodeId?: string;
    }
  | {
      kind: "correlation";
      id: string;
      title: string;
      subtitle: string;
      correlationId: string;
    };

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

      const events = eventsQuery.data ?? runtimeEvents;
      const runs = functionsQuery.data ?? functionRuns;

      const storyResults: SearchResult[] = runtimeStories.flatMap((story) => {
        const matchesStory = [
          story.id,
          story.name,
          story.service,
          story.source,
          story.correlationId,
        ].some((value) => value.toLowerCase().includes(normalized));

        const matchingNodes = story.nodes.filter((node) =>
          [node.id, node.name, node.service, node.kind].some((value) =>
            value.toLowerCase().includes(normalized)
          )
        );

        return [
          ...(matchesStory
            ? [
                {
                  kind: "story" as const,
                  id: story.id,
                  title: story.name,
                  subtitle: `${story.service}/${story.source}`,
                  correlationId: story.correlationId,
                  storyId: story.id,
                },
              ]
            : []),
          ...matchingNodes.map<SearchResult>((node) => ({
            kind: "story",
            id: node.id,
            title: node.name,
            subtitle: `${story.id} · ${node.service}`,
            correlationId: story.correlationId,
            storyId: story.id,
            nodeId: node.id,
          })),
        ];
      });

      const eventResults: SearchResult[] = events
        .filter((event) =>
          [event.id, event.eventName, event.correlationId].some((value) =>
            value.toLowerCase().includes(normalized)
          )
        )
        .map((event) => ({
          kind: "event",
          id: event.id,
          title: event.eventName,
          subtitle: event.id,
          correlationId: event.correlationId,
          record: { kind: "event", item: event },
        }));

      const functionResults: SearchResult[] = runs
        .filter((run) =>
          [run.id, run.functionName, run.correlationId].some((value) =>
            value.toLowerCase().includes(normalized)
          )
        )
        .map((run) => ({
          kind: "function",
          id: run.id,
          title: run.functionName,
          subtitle: run.id,
          correlationId: run.correlationId,
          record: { kind: "function", item: run },
        }));

      const correlations = Array.from(
        new Set([
          ...runtimeEvents.map((event) => event.correlationId),
          ...events.map((event) => event.correlationId),
          ...runs.map((run) => run.correlationId),
          ...timelineItems.map((item) => item.correlationId),
        ])
      )
        .filter((id) => id.toLowerCase().includes(normalized))
        .map<SearchResult>((id) => ({
          kind: "correlation",
          id,
          title: id,
          subtitle: "Open correlation in Stories",
          correlationId: id,
        }));

      return [
        ...storyResults,
        ...correlations,
        ...eventResults,
        ...functionResults,
      ].slice(0, 8);
    },
    [eventsQuery.data, functionsQuery.data]
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
