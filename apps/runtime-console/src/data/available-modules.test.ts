import { describe, expect, test } from "vitest";

import {
  availableModulesPanelState,
  availableModulesQueryKey,
  availableModulesRows,
  moduleRefreshInvalidationQueryKeys,
  fetchAvailableModules,
  sampleAvailableModulesResponse,
} from "./available-modules";

describe("available modules provider", () => {
  test("provides read-only rows derived from available module data", () => {
    expect(sampleAvailableModulesResponse.version).toBe(1);
    expect(sampleAvailableModulesResponse.catalog.registryFile).toBe(
      ".lenso/module-registry.json"
    );

    expect(availableModulesRows()).toEqual([
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

  test("defines a stable async fetch boundary for available modules", async () => {
    await expect(fetchAvailableModules()).resolves.toBe(
      sampleAvailableModulesResponse
    );
    expect(availableModulesQueryKey).toEqual(["modules", "available-modules"]);
  });

  test("includes available modules in module refresh invalidation keys", () => {
    expect(moduleRefreshInvalidationQueryKeys()).toEqual([
      ["modules", "registry"],
      availableModulesQueryKey,
    ]);
  });

  test("fetches the current available modules endpoint in API mode", async () => {
    const getCalls: string[] = [];
    const response = {
      ...sampleAvailableModulesResponse,
      status: "passed",
    };
    const client = {
      get(path: string) {
        getCalls.push(path);
        return {
          json: async () => response,
        };
      },
    };

    await expect(
      fetchAvailableModules({ apiMode: true, client })
    ).resolves.toBe(response);
    expect(getCalls).toEqual(["admin/data/module-registry/snapshot"]);
  });

  test("summarizes available modules panel states", () => {
    expect(
      availableModulesPanelState({
        isError: false,
        isLoading: true,
        rows: [],
      })
    ).toEqual({
      moduleCount: 0,
      kind: "loading",
      label: "loading",
      message: "Loading available modules.",
    });

    expect(
      availableModulesPanelState({
        isError: true,
        isLoading: false,
        rows: [],
      })
    ).toMatchObject({
      kind: "error",
      label: "unavailable",
    });

    expect(
      availableModulesPanelState({
        isError: false,
        isLoading: false,
        rows: [],
      })
    ).toMatchObject({
      kind: "empty",
      label: "no remote modules",
    });

    expect(
      availableModulesPanelState({
        isError: false,
        isLoading: false,
        rows: availableModulesRows(),
      })
    ).toMatchObject({
      moduleCount: 2,
      kind: "ready",
      label: "2 modules",
    });
  });
});
