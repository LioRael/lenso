import { identityConsoleManifest } from "@lenso/identity-console";
import { storyConsoleManifest } from "@lenso/story-console";

export const consolePackageManifests = [
  identityConsoleManifest,
  storyConsoleManifest,
] as const;

export const consolePackageNames = consolePackageManifests.map(
  (manifest) => manifest.packageName
);
