import { defineConsolePackageManifest } from "@lenso/runtime-console-api";

export const exampleConsoleManifest = defineConsolePackageManifest({
  area: "runtime",
  exportName: "exampleConsoleModule",
  icon: "activity",
  id: "example-console",
  label: "Example",
  packageName: "@lenso/example-console",
  requiredCapabilities: [],
  route: "/runtime/example-console",
  surfaceName: "example",
} as const);
