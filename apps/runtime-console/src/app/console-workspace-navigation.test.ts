import { describe, expect, test } from "vitest";

import type { ConsoleNavigationItem } from "./console-module-api";
import {
  activeWorkspaceIdForPath,
  buildWorkspaceNavigation,
  SYSTEM_WORKSPACE,
} from "./console-workspace-navigation";

const items: ConsoleNavigationItem[] = [
  {
    icon: "activity",
    label: "Overview",
    moduleId: "host",
    navigation: {
      order: 0,
      workspace: SYSTEM_WORKSPACE,
    },
    path: "/overview",
  },
  {
    icon: "database",
    label: "Contacts",
    moduleId: "crm",
    navigation: {
      group: {
        id: "customers",
        label: "Customers",
        order: 20,
      },
      order: 10,
      workspace: {
        icon: "database",
        id: "crm",
        label: "CRM",
      },
    },
    path: "/crm/contacts",
  },
  {
    icon: "database",
    label: "Deals",
    moduleId: "crm",
    navigation: {
      group: {
        id: "pipeline",
        label: "Pipeline",
        order: 10,
      },
      order: 10,
      workspace: {
        icon: "database",
        id: "crm",
        label: "CRM",
      },
    },
    path: "/crm/deals",
  },
];

describe("console workspace navigation", () => {
  test("builds system and module-declared workspaces", () => {
    expect(buildWorkspaceNavigation(items)).toMatchObject([
      {
        id: "system",
        items: [
          {
            label: "Overview",
            path: "/overview",
          },
        ],
        label: "System",
      },
      {
        groups: [
          {
            id: "pipeline",
            items: [
              {
                label: "Deals",
                path: "/crm/deals",
              },
            ],
            label: "Pipeline",
          },
          {
            id: "customers",
            items: [
              {
                label: "Contacts",
                path: "/crm/contacts",
              },
            ],
            label: "Customers",
          },
        ],
        id: "crm",
        label: "CRM",
      },
    ]);
  });

  test("selects active workspace from route path", () => {
    const workspaces = buildWorkspaceNavigation(items);

    expect(activeWorkspaceIdForPath(workspaces, "/crm/contacts")).toBe("crm");
    expect(activeWorkspaceIdForPath(workspaces, "/unknown")).toBe("system");
  });
});
