import { describe, expect, test } from "vitest";

import {
  availableModuleRegistrySnapshotPanelState,
  availableModuleRegistrySnapshotQueryKey,
  availableModuleRegistrySnapshotRows,
  moduleRefreshInvalidationQueryKeys,
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

  test("includes registry snapshot in module refresh invalidation keys", () => {
    expect(moduleRefreshInvalidationQueryKeys()).toEqual([
      ["modules", "registry"],
      availableModuleRegistrySnapshotQueryKey,
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

  test("summarizes registry snapshot panel states", () => {
    expect(
      availableModuleRegistrySnapshotPanelState({
        isError: false,
        isLoading: true,
        rows: [],
      })
    ).toEqual({
      issueCount: 0,
      kind: "loading",
      label: "loading snapshot",
      message: "Fetching registry preflight snapshot.",
    });

    expect(
      availableModuleRegistrySnapshotPanelState({
        isError: true,
        isLoading: false,
        rows: [],
      })
    ).toMatchObject({
      kind: "error",
      label: "snapshot unavailable",
    });

    expect(
      availableModuleRegistrySnapshotPanelState({
        isError: false,
        isLoading: false,
        rows: [],
      })
    ).toMatchObject({
      kind: "empty",
      label: "no remote modules",
    });

    expect(
      availableModuleRegistrySnapshotPanelState({
        isError: false,
        isLoading: false,
        rows: availableModuleRegistrySnapshotRows(),
      })
    ).toMatchObject({
      issueCount: 1,
      kind: "issues",
      label: "1 issue",
    });

    expect(
      availableModuleRegistrySnapshotPanelState({
        isError: false,
        isLoading: false,
        rows: availableModuleRegistrySnapshotRows({
          ...sampleAvailableModuleRegistrySnapshot,
          issues: [],
          modules: [sampleAvailableModuleRegistrySnapshot.modules[0]!],
          status: "passed",
        }),
      })
    ).toMatchObject({
      issueCount: 0,
      kind: "ready",
      label: "ready",
    });
  });
});
