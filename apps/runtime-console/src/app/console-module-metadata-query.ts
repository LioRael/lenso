import { useQuery } from "@tanstack/react-query";

import { httpClient, isApiMode } from "../lib/http-client";
import type { ConsoleModuleMetadata } from "./console-module-resolver";

type ModulesMetadataResponse = {
  modules: ConsoleModuleMetadata[];
};

const consoleModulesMetadataQueryKey = ["modules", "registry"] as const;

export function useConsoleModulesMetadata() {
  return useQuery({
    enabled: isApiMode(),
    queryKey: consoleModulesMetadataQueryKey,
    queryFn: () =>
      httpClient.get("admin/data/modules").json<ModulesMetadataResponse>(),
  });
}
