import { defineConsolePackageManifest } from "@lenso/runtime-console-api";

import storyConsoleSurface from "../console-surface.json";

const storyConsoleSurfaceContract = storyConsoleSurface as unknown as {
  readonly area: "runtime";
  readonly exportName: "storyConsoleModule";
  readonly icon: "workflow";
  readonly id: "platform-story";
  readonly label: "Stories";
  readonly packageName: "@lenso/story-console";
  readonly requiredCapabilities: readonly ["runtime.stories.read"];
  readonly route: "/runtime/stories";
  readonly source: "first_party";
  readonly surfaceName: "stories";
  readonly version: "workspace";
};

export const storyConsoleManifest = defineConsolePackageManifest(
  storyConsoleSurfaceContract
);
