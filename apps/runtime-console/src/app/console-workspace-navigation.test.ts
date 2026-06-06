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

  test("defaults items without navigation to the system workspace", () => {
    expect(
      buildWorkspaceNavigation([
        {
          icon: "activity",
          label: "Defaulted Overview",
          moduleId: "host",
          path: "/defaulted-overview",
        },
      ])
    ).toMatchObject([
      {
        id: "system",
        items: [
          {
            label: "Defaulted Overview",
            path: "/defaulted-overview",
          },
        ],
        label: "System",
      },
    ]);
  });

  test("keeps groupless module items in workspace items", () => {
    expect(
      buildWorkspaceNavigation([
        {
          icon: "database",
          label: "CRM Home",
          moduleId: "crm",
          navigation: {
            workspace: {
              icon: "database",
              id: "crm",
              label: "CRM",
            },
          },
          path: "/crm",
        },
      ])
    ).toMatchObject([
      {
        groups: [],
        id: "crm",
        items: [
          {
            label: "CRM Home",
            path: "/crm",
          },
        ],
      },
    ]);
  });

  test("sorts unordered items by missing order, label, and path", () => {
    expect(
      buildWorkspaceNavigation([
        {
          label: "Beta",
          moduleId: "system",
          path: "/beta",
        },
        {
          label: "Alpha",
          moduleId: "system",
          path: "/alpha-secondary",
        },
        {
          label: "Alpha",
          moduleId: "system",
          path: "/alpha-primary",
        },
      ])[0]?.items.map((item) => item.path)
    ).toEqual(["/alpha-primary", "/alpha-secondary", "/beta"]);
  });

  test("does not mutate the input item array", () => {
    const unorderedItems: ConsoleNavigationItem[] = [
      {
        label: "Beta",
        moduleId: "system",
        path: "/beta",
      },
      {
        label: "Alpha",
        moduleId: "system",
        path: "/alpha",
      },
    ];
    const originalOrder = unorderedItems.map((item) => item.path);

    expect(buildWorkspaceNavigation(unorderedItems)[0]?.items).toMatchObject([
      {
        path: "/alpha",
      },
      {
        path: "/beta",
      },
    ]);
    expect(unorderedItems.map((item) => item.path)).toEqual(originalOrder);
  });

  test("selects active workspace from route path", () => {
    const workspaces = buildWorkspaceNavigation(items);

    expect(activeWorkspaceIdForPath(workspaces, "/crm/contacts")).toBe("crm");
    expect(activeWorkspaceIdForPath(workspaces, "/unknown")).toBe("system");
  });

  test("selects active workspace from child route paths", () => {
    const workspaces = buildWorkspaceNavigation(items);

    expect(activeWorkspaceIdForPath(workspaces, "/crm/contacts/123")).toBe(
      "crm"
    );
  });

  test("does not match sibling path prefixes as child routes", () => {
    const workspaces = buildWorkspaceNavigation([
      {
        label: "CRM",
        moduleId: "crm",
        navigation: {
          workspace: {
            icon: "database",
            id: "crm",
            label: "CRM",
          },
        },
        path: "/crm",
      },
    ]);

    expect(activeWorkspaceIdForPath(workspaces, "/crm-other")).toBe("system");
  });

  test("selects the longest matching child route prefix", () => {
    const workspaces = buildWorkspaceNavigation([
      {
        label: "CRM",
        moduleId: "crm",
        navigation: {
          workspace: {
            icon: "database",
            id: "crm",
            label: "CRM",
          },
        },
        path: "/crm",
      },
      {
        label: "Contacts",
        moduleId: "contacts",
        navigation: {
          workspace: {
            icon: "database",
            id: "contacts",
            label: "Contacts",
          },
        },
        path: "/crm/contacts",
      },
    ]);

    expect(activeWorkspaceIdForPath(workspaces, "/crm/contacts/123")).toBe(
      "contacts"
    );
  });
});
