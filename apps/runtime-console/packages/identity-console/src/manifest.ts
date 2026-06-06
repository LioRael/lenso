import { defineConsolePackageManifest } from "@lenso/runtime-console-api";

export const identityConsoleManifest = defineConsolePackageManifest({
  area: "data",
  exportName: "identityConsoleModule",
  icon: "database",
  id: "identity",
  label: "Identity",
  packageName: "@lenso/identity-console",
  requiredCapabilities: ["identity.users.read"],
  route: "/data/identity",
  source: "installed",
  surfaceName: "identity",
  version: "workspace",
} as const);
