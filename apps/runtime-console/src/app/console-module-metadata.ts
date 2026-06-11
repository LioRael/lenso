import { isApiMode } from "../lib/http-client";
import { useConsoleCapabilities } from "./console-capabilities";
import { useConsoleModulesMetadata } from "./console-module-metadata-query";
import {
  type ConsoleModuleMetadata,
  createDevManualConsolePackageInstaller,
  missingConsolePackageReferences,
  planConsolePackageInstall,
  resolveConsoleModules,
  selectConsoleModulePackageReferences,
} from "./console-module-resolver";
import {
  buildConsoleNavigation,
  buildTimeConsoleModuleMetadata,
} from "./console-modules";

export function consoleModuleMetadataWithFallback({
  apiMode,
  data,
  isError,
  isPending,
}: {
  apiMode: boolean;
  data?: ConsoleModuleMetadata[] | undefined;
  isError: boolean;
  isPending: boolean;
}): ConsoleModuleMetadata[] {
  if (data) {
    return data;
  }
  return apiMode && !(isError || isPending)
    ? []
    : buildTimeConsoleModuleMetadata;
}

export function navigationFromConsoleModuleMetadata(
  modules: ConsoleModuleMetadata[],
  availableCapabilities: readonly string[]
) {
  return buildConsoleNavigation(
    resolveConsoleModules(
      selectConsoleModulePackageReferences(modules, { availableCapabilities })
    )
  );
}

export function missingConsolePackagesFromMetadata(
  modules: ConsoleModuleMetadata[]
) {
  return missingConsolePackageReferences(modules);
}

export function consolePackageInstallPlanFromMetadata(
  modules: ConsoleModuleMetadata[]
) {
  return planConsolePackageInstall(missingConsolePackagesFromMetadata(modules));
}

export async function previewConsolePackageInstallResults(
  modules: ConsoleModuleMetadata[]
) {
  const installer = createDevManualConsolePackageInstaller();
  return Promise.all(
    consolePackageInstallPlanFromMetadata(modules).map((plan) =>
      installer.install(plan)
    )
  );
}

export function useConsoleNavigation() {
  const apiMode = isApiMode();
  const availableCapabilities = useConsoleCapabilities();
  const modulesQuery = useConsoleModulesMetadata();
  const modules = consoleModuleMetadataWithFallback({
    apiMode,
    data: modulesQuery.data?.modules,
    isError: modulesQuery.isError,
    isPending: modulesQuery.isPending,
  });

  return navigationFromConsoleModuleMetadata(modules, availableCapabilities);
}
