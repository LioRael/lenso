import { httpClient, isApiMode } from "../lib/http-client";
import {
  type AvailableModuleRegistryDoctorSnapshot,
  type AvailableModuleRegistryRow,
  availableModuleRegistryRowsFromDoctorSnapshot,
} from "../pages/available-module-registry-model";

export const sampleAvailableModuleRegistrySnapshot = {
  catalog: {
    modules: 2,
    registryFile: ".lenso/module-registry.json",
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
} satisfies AvailableModuleRegistryDoctorSnapshot;

export const availableModuleRegistrySnapshotQueryKey = [
  "modules",
  "available-registry-snapshot",
] as const;

export function moduleRefreshInvalidationQueryKeys() {
  return [
    ["modules", "registry"],
    availableModuleRegistrySnapshotQueryKey,
  ] as const;
}

type RegistrySnapshotHttpClient = {
  get: (path: string) => {
    json: () => Promise<AvailableModuleRegistryDoctorSnapshot>;
  };
};

export async function fetchAvailableModuleRegistrySnapshot({
  apiMode = isApiMode(),
  client = httpClient,
}: {
  apiMode?: boolean;
  client?: RegistrySnapshotHttpClient;
} = {}): Promise<AvailableModuleRegistryDoctorSnapshot> {
  if (apiMode) {
    return client.get("admin/data/module-registry/snapshot").json();
  }
  return sampleAvailableModuleRegistrySnapshot;
}

export function availableModuleRegistrySnapshotRows(
  snapshot: AvailableModuleRegistryDoctorSnapshot = sampleAvailableModuleRegistrySnapshot
) {
  return availableModuleRegistryRowsFromDoctorSnapshot(snapshot);
}

export type AvailableModuleRegistrySnapshotPanelState = {
  kind: "loading" | "error" | "empty" | "issues" | "ready";
  label: string;
  message: string;
  issueCount: number;
};

export function availableModuleRegistrySnapshotPanelState({
  isError,
  isLoading,
  rows,
}: {
  isError: boolean;
  isLoading: boolean;
  rows: AvailableModuleRegistryRow[];
}): AvailableModuleRegistrySnapshotPanelState {
  if (isLoading) {
    return {
      issueCount: 0,
      kind: "loading",
      label: "loading snapshot",
      message: "Fetching registry preflight snapshot.",
    };
  }
  if (isError) {
    return {
      issueCount: 0,
      kind: "error",
      label: "snapshot unavailable",
      message: "Registry preflight snapshot could not be loaded.",
    };
  }
  if (rows.length === 0) {
    return {
      issueCount: 0,
      kind: "empty",
      label: "no remote modules",
      message: "No remote modules are present in the registry snapshot.",
    };
  }

  const issueCount = rows.filter(
    (row) => row.preflightStatus !== "ready"
  ).length;
  if (issueCount > 0) {
    return {
      issueCount,
      kind: "issues",
      label: `${issueCount} issue${issueCount === 1 ? "" : "s"}`,
      message: "Registry snapshot has modules that need attention.",
    };
  }

  return {
    issueCount: 0,
    kind: "ready",
    label: "ready",
    message: "Registry snapshot preflight is ready.",
  };
}
