import { defineConsoleModule } from "../../app/console-module-api";
import { storyConsoleManifest } from "./manifest";
import { RuntimeStoriesPage } from "./page";

export const storyConsoleModule = defineConsoleModule({
  id: storyConsoleManifest.id,
  surfaces: [
    {
      area: storyConsoleManifest.area,
      component: RuntimeStoriesPage,
      icon: storyConsoleManifest.icon,
      label: storyConsoleManifest.label,
      path: storyConsoleManifest.route,
    },
  ],
});

export { storyConsoleManifest } from "./manifest";
export { RuntimeStoriesPage, runtimeStoriesDefaultViewMode } from "./page";
export { shouldCloseInspectorOnEscape } from "./keyboard";
export {
  resizeExecutionInspectorLayout,
  resizeExecutionInspectorWidth,
  resizeServicesPanelHeight,
  resizeServicesPanelLayout,
  resizeStoryListWidth,
  runtimeStoriesLayoutDefaults,
} from "./layout";
export { resolveSelectedRuntimeStory } from "./selection";
export {
  pushRuntimeStoriesUrl,
  readExecutionInspectorTab,
  readRuntimeStoriesParam,
  readStoryViewMode,
  replaceRuntimeStoriesUrl,
  runtimeStoriesPath,
  storyUrlId,
} from "./url-model";
