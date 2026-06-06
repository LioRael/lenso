import { exampleConsoleManifest } from "@lenso/example-console";
import { storyConsoleManifest } from "@lenso/story-console";

export const consolePackageManifests = [
  exampleConsoleManifest,
  storyConsoleManifest,
] as const;

export const consolePackageNames = consolePackageManifests.map(
  (manifest) => manifest.packageName
);
