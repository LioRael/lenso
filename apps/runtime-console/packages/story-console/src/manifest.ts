import { defineConsolePackageManifest } from "@lenso/runtime-console-api";

import storyConsoleSurface from "../console-surface.json";

const storyConsoleSurfaceContract = storyConsoleSurface as unknown as {
  readonly area: "runtime";
  readonly exportName: "storyConsoleModule";
  readonly icon: "workflow";
  readonly id: "platform-story";
  readonly label: "Stories";
  readonly navigation: {
    readonly order: 20;
    readonly workspace: {
      readonly icon: "settings";
      readonly id: "system";
      readonly label: "System";
    };
  };
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
