import { ExecutionInspector } from "../components/runtime/execution-inspector";
import {
  defaultExecutionInspectorTab,
  executionInspectorTabs,
} from "../components/runtime/execution-inspector-model";
import { ResizeHandle } from "../components/runtime/resize-handle";
import { useRuntimeConsole } from "../components/runtime/runtime-console-context";
import { findStoryByCorrelation } from "../components/runtime/runtime-story-target";
import { RuntimeStoryVisualization } from "../components/runtime/runtime-story-visualization";
import { ServiceSummaryStrip } from "../components/runtime/service-summary-strip";
import { StoryHeader } from "../components/runtime/story-header";
import { StoryList } from "../components/runtime/story-list";
import { EmptyState } from "../components/ui/empty-state";
import { retryTargetForNode, runtimeStories } from "../data/mock-runtime";
import {
  useBrowserUrlPopState,
  writeBrowserUrl,
} from "../hooks/use-browser-url-state";
import { useListKeyboard } from "../hooks/use-list-keyboard";
import { usePersistedLayout } from "../hooks/use-persisted-layout";
import { useRuntimeStories } from "../hooks/use-runtime-queries";
import { operationsPath } from "../pages/operations-url-model";

export const runtimeConsoleHostApi = {
  context: {
    useRuntimeConsole,
  },
  data: {
    retryTargetForNode,
    runtimeStories,
  },
  hooks: {
    useBrowserUrlPopState,
    useListKeyboard,
    usePersistedLayout,
    writeBrowserUrl,
  },
  queries: {
    useRuntimeStories,
  },
  routing: {
    buildPath: operationsPath,
  },
  story: {
    executionInspectorTabs,
    findStoryByCorrelation,
  },
  ui: {
    common: {
      EmptyState,
    },
    runtime: {
      ExecutionInspector,
      ResizeHandle,
      RuntimeStoryVisualization,
      ServiceSummaryStrip,
      StoryHeader,
      StoryList,
      defaultExecutionInspectorTab,
    },
  },
};

export type RuntimeConsoleHostApi = typeof runtimeConsoleHostApi;
export type { ExecutionInspectorTab } from "../components/runtime/execution-inspector-model";
export type { StoryViewMode } from "../components/runtime/story-tabs";
export type { ExecutionNode, RuntimeStory } from "../data/mock-runtime";
