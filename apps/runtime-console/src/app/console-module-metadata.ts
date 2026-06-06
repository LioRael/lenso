import { useQuery } from "@tanstack/react-query";

import { httpClient, isApiMode } from "../lib/http-client";
import {
  type ConsoleModuleMetadata,
  resolveConsoleModules,
  selectConsoleModulePackageReferences,
} from "./console-module-resolver";
import {
  buildConsoleNavigation,
  buildTimeConsoleModuleMetadata,
} from "./console-modules";

type ModulesMetadataResponse = {
  modules: ConsoleModuleMetadata[];
};

const consoleModulesMetadataQueryKey = ["modules", "registry"] as const;

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
  modules: ConsoleModuleMetadata[]
) {
  return buildConsoleNavigation(
    resolveConsoleModules(selectConsoleModulePackageReferences(modules))
  );
}

export function useConsoleNavigation() {
  const apiMode = isApiMode();
  const modulesQuery = useQuery({
    enabled: apiMode,
    queryKey: consoleModulesMetadataQueryKey,
    queryFn: () =>
      httpClient.get("admin/data/modules").json<ModulesMetadataResponse>(),
  });
  const modules = consoleModuleMetadataWithFallback({
    apiMode,
    data: modulesQuery.data?.modules,
    isError: modulesQuery.isError,
    isPending: modulesQuery.isPending,
  });

  return navigationFromConsoleModuleMetadata(modules);
}
