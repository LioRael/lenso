import { describe, expect, test } from "vitest";

import {
  availableModuleRegistrySnapshotRows,
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
});
