import { describe, expect, test } from "vitest";

import {
  availableModuleRegistrySnapshotQueryKey,
  availableModuleRegistrySnapshotRows,
  fetchAvailableModuleRegistrySnapshot,
  sampleAvailableModuleRegistrySnapshot,
} from "./available-module-registry-snapshot";

describe("available module registry snapshot provider", () => {
  test("provides read-only rows derived from the registry doctor snapshot", () => {
    expect(sampleAvailableModuleRegistrySnapshot.version).toBe(1);
    expect(sampleAvailableModuleRegistrySnapshot.catalog.registryFile).toBe(
      ".lenso/module-registry.json"
    );

    expect(availableModuleRegistrySnapshotRows()).toEqual([
      expect.objectContaining({
        name: "billing",
        preflightStatus: "ready",
        source: "remote",
      }),
      expect.objectContaining({
        name: "local-crm",
        preflightReason: "local-crm baseUrl is missing",
        preflightStatus: "needs_base_url",
      }),
    ]);
  });

  test("defines a stable async fetch boundary for future registry sources", async () => {
    await expect(fetchAvailableModuleRegistrySnapshot()).resolves.toBe(
      sampleAvailableModuleRegistrySnapshot
    );
    expect(availableModuleRegistrySnapshotQueryKey).toEqual([
      "modules",
      "available-registry-snapshot",
    ]);
  });

  test("fetches the registry snapshot endpoint in API mode", async () => {
    const getCalls: string[] = [];
    const snapshot = {
      ...sampleAvailableModuleRegistrySnapshot,
      status: "passed",
    };
    const client = {
      get(path: string) {
        getCalls.push(path);
        return {
          json: async () => snapshot,
        };
      },
    };

    await expect(
      fetchAvailableModuleRegistrySnapshot({ apiMode: true, client })
    ).resolves.toBe(snapshot);
    expect(getCalls).toEqual(["admin/data/module-registry/snapshot"]);
  });
});
