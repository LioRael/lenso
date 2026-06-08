import { identityConsoleManifest } from "@lenso/identity-console";
import { remoteCrmConsoleManifest } from "@lenso/remote-crm-console";
import { storyConsoleManifest } from "@lenso/story-console";

export const consolePackageManifests = [
  identityConsoleManifest,
  remoteCrmConsoleManifest,
  storyConsoleManifest,
] as const;

export const consolePackageNames = consolePackageManifests.map(
  (manifest) => manifest.packageName
);
