import { defineConsolePackageManifest } from "@lenso/runtime-console-api";

export const storyConsoleManifest = defineConsolePackageManifest({
  area: "runtime",
  exportName: "storyConsoleModule",
  icon: "workflow",
  id: "platform-story",
  label: "Stories",
  packageName: "@lenso/story-console",
  requiredCapabilities: ["runtime.stories.read"],
  route: "/runtime/stories",
  source: "first_party",
  surfaceName: "stories",
  version: "workspace",
} as const);
