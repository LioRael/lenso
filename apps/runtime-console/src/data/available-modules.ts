import { httpClient, isApiMode } from "../lib/http-client";
import {
  type AvailableModulesResponse,
  type AvailableModuleRow,
  availableModuleRowsFromResponse,
} from "../pages/available-modules-model";

export const sampleAvailableModulesResponse = {
  catalog: {
    modules: 2,
    registryFile: ".lenso/module-catalog.json",
    version: 1,
  },
  issues: [
    {
      fix: "add baseUrl or use a manifest URL ending with /manifest",
      group: "Catalog",
      message: "local-crm baseUrl is missing",
    },
  ],
  modules: [
    {
      baseUrl: "https://example.com/lenso/module/v1",
      catalogVersion: "0.1.0",
      consolePackageHints: 1,
      manifestName: "billing",
      manifestReference: "https://example.com/lenso/module/v1/manifest",
      manifestStatus: "ok",
      manifestVersion: "0.1.0",
      name: "billing",
      source: "remote",
      status: "ready",
    },
    {
      baseUrl: null,
      catalogVersion: "0.1.0",
      consolePackageHints: 0,
      manifestName: "local-crm",
      manifestReference: "./lenso.module.json",
      manifestStatus: "ok",
      manifestVersion: "0.1.0",
      name: "local-crm",
      source: "remote",
      status: "needs_attention",
    },
  ],
  status: "failed",
  version: 1,
} satisfies AvailableModulesResponse;

export const availableModulesQueryKey = [
  "modules",
  "available-modules",
] as const;

export function moduleRefreshInvalidationQueryKeys() {
  return [["modules", "registry"], availableModulesQueryKey] as const;
}

type AvailableModulesHttpClient = {
  get: (path: string) => {
    json: () => Promise<AvailableModulesResponse>;
  };
};

export async function fetchAvailableModules({
  apiMode = isApiMode(),
  client = httpClient,
}: {
  apiMode?: boolean;
  client?: AvailableModulesHttpClient;
} = {}): Promise<AvailableModulesResponse> {
  if (apiMode) {
    return client.get("admin/data/available-modules").json();
  }
  return sampleAvailableModulesResponse;
}

export function availableModulesRows(
  response: AvailableModulesResponse = sampleAvailableModulesResponse
) {
  return availableModuleRowsFromResponse(response);
}

export type AvailableModulesPanelState = {
  actionCommand: string;
  detail: string;
  kind: "loading" | "error" | "empty" | "ready";
  label: string;
  message: string;
  moduleCount: number;
  source: string;
};

export function availableModulesPanelState({
  isError,
  isLoading,
  response,
  rows,
}: {
  isError: boolean;
  isLoading: boolean;
  response?: AvailableModulesResponse | null;
  rows: AvailableModuleRow[];
}): AvailableModulesPanelState {
  const source = response?.catalog.registryFile ?? ".lenso/module-catalog.json";
  const firstIssue = response?.issues[0];
  if (isLoading) {
    return {
      actionCommand: "lenso module catalog add <manifest-url>",
      detail: source,
      moduleCount: 0,
      kind: "loading",
      label: "loading",
      message: "Loading available modules.",
      source,
    };
  }
  if (isError) {
    return {
      actionCommand: "lenso module catalog add <manifest-url>",
      detail: "check the API and local catalog file",
      moduleCount: 0,
      kind: "error",
      label: "unavailable",
      message: "Available modules could not be loaded.",
      source,
    };
  }
  if (rows.length === 0) {
    return {
      actionCommand: "lenso module catalog add <manifest-url>",
      detail: "add a manifest URL to show modules here",
      moduleCount: 0,
      kind: "empty",
      label: "no remote modules",
      message: `No modules in ${source}.`,
      source,
    };
  }

  return {
    actionCommand: "lenso module catalog add <manifest-url>",
    detail: firstIssue?.fix ?? "copy install commands from this catalog",
    moduleCount: rows.length,
    kind: "ready",
    label: `${rows.length} module${rows.length === 1 ? "" : "s"}`,
    message: `Catalog: ${source}`,
    source,
  };
}
