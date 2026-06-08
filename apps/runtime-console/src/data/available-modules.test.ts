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
      ".lenso/module-catalog.json"
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
    expect(getCalls).toEqual(["admin/data/available-modules"]);
  });

  test("summarizes available modules panel states", () => {
    expect(
      availableModulesPanelState({
        isError: false,
        isLoading: true,
        response: null,
        rows: [],
      })
    ).toEqual({
      actionCommand: "lenso module catalog add <manifest-url>",
      detail: ".lenso/module-catalog.json",
      moduleCount: 0,
      kind: "loading",
      label: "loading",
      message: "Loading available modules.",
      source: ".lenso/module-catalog.json",
    });

    expect(
      availableModulesPanelState({
        isError: true,
        isLoading: false,
        response: null,
        rows: [],
      })
    ).toMatchObject({
      actionCommand: "lenso module catalog add <manifest-url>",
      detail: "check the API and local catalog file",
      kind: "error",
      label: "unavailable",
      source: ".lenso/module-catalog.json",
    });

    expect(
      availableModulesPanelState({
        isError: false,
        isLoading: false,
        response: {
          ...sampleAvailableModulesResponse,
          catalog: {
            modules: 0,
            registryFile: ".lenso/module-catalog.json",
            version: 1,
          },
          modules: [],
        },
        rows: [],
      })
    ).toMatchObject({
      actionCommand: "lenso module catalog add <manifest-url>",
      detail: "add a manifest URL to show modules here",
      kind: "empty",
      label: "no remote modules",
      message: "No modules in .lenso/module-catalog.json.",
    });

    expect(
      availableModulesPanelState({
        isError: false,
        isLoading: false,
        response: sampleAvailableModulesResponse,
        rows: availableModulesRows(),
      })
    ).toMatchObject({
      actionCommand: "lenso module catalog add <manifest-url>",
      detail: "add baseUrl or use a manifest URL ending with /manifest",
      moduleCount: 2,
      kind: "ready",
      label: "2 modules",
      message: "Catalog: .lenso/module-catalog.json",
      source: ".lenso/module-catalog.json",
    });
  });
});
