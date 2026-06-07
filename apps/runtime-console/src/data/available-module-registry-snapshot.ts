import {
  type AvailableModuleRegistryDoctorSnapshot,
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

export async function fetchAvailableModuleRegistrySnapshot(): Promise<AvailableModuleRegistryDoctorSnapshot> {
  return sampleAvailableModuleRegistrySnapshot;
}

export function availableModuleRegistrySnapshotRows(
  snapshot: AvailableModuleRegistryDoctorSnapshot = sampleAvailableModuleRegistrySnapshot
) {
  return availableModuleRegistryRowsFromDoctorSnapshot(snapshot);
}
