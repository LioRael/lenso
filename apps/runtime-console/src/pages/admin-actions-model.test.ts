import { describe, expect, test } from "vitest";

import type { RuntimeAdminActionInvocation } from "../hooks/use-runtime-queries";
import {
  adminActionInspectorDetails,
  adminActionPrimarySummary,
  adminActionResultLabel,
  adminActionsPath,
  aggregateAdminActionInvocations,
  filterAdminActionInvocations,
  flattenAdminActionInvocationPages,
  nextAdminActionInvocationCursor,
  summarizeAdminActionInvocations,
} from "./admin-actions-model";

const actions = [
  adminAction({
    duration_ms: 100,
    id: "admin_a",
    module_name: "remote-crm",
    occurred_at: "2026-06-03T10:00:00.000Z",
    success: true,
  }),
  adminAction({
    action_name: "rebuild_index",
    capability: "identity.users.maintain",
    duration_ms: 500,
    error_code: "action_validation_failed",
    error_message: "window too wide",
    id: "admin_b",
    label: "Rebuild user index",
    module_name: "identity",
    occurred_at: "2026-06-03T10:05:00.000Z",
    success: false,
  }),
  adminAction({
    action_name: "sync_contacts",
    duration_ms: 300,
    error_code: "remote_timeout",
    id: "admin_c",
    module_name: "remote-crm",
    occurred_at: "2026-06-03T09:55:00.000Z",
    success: false,
  }),
];

describe("admin actions model", () => {
  test("filters by result and sorts newest first", () => {
    expect(
      filterAdminActionInvocations(actions, {
        query: "",
        result: "failed",
      }).map((action) => action.id)
    ).toEqual(["admin_b", "admin_c"]);
  });

  test("searches operational identifiers and summaries", () => {
    expect(
      filterAdminActionInvocations(actions, {
        query: "identity validation",
        result: "all",
      }).map((action) => action.id)
    ).toEqual(["admin_b"]);
  });

  test("summarizes result counts and average duration", () => {
    expect(summarizeAdminActionInvocations(actions)).toEqual({
      avgDurationMs: 300,
      failed: 2,
      success: 1,
      total: 3,
    });
  });

  test("aggregates failure pressure by module and action", () => {
    expect(aggregateAdminActionInvocations(actions, "module", 2)).toEqual([
      {
        failed: 1,
        failureRate: 1 / 2,
        key: "remote-crm",
        total: 2,
      },
      {
        failed: 1,
        failureRate: 1,
        key: "identity",
        total: 1,
      },
    ]);

    expect(
      aggregateAdminActionInvocations(actions, "action", 2).map(
        (row) => row.key
      )
    ).toEqual(["sync_contacts", "rebuild_index"]);
  });

  test("aggregates successful rows under success for error grouping", () => {
    expect(
      aggregateAdminActionInvocations(actions, "error", 3).map((row) => row.key)
    ).toEqual(["action_validation_failed", "remote_timeout", "success"]);
  });

  test("flattens loaded cursor pages", () => {
    expect(
      flattenAdminActionInvocationPages([
        {
          data: [actions[0]!],
          page: {
            limit: 1,
            next_created_before: actions[0]!.occurred_at,
          },
        },
        {
          data: [actions[1]!],
          page: {
            limit: 1,
            next_created_before: null,
          },
        },
      ]).map((action) => action.id)
    ).toEqual(["admin_a", "admin_b"]);
  });

  test("reads the next cursor from the latest loaded page", () => {
    expect(
      nextAdminActionInvocationCursor([
        {
          data: [actions[0]!],
          page: {
            limit: 1,
            next_created_before: actions[0]!.occurred_at,
          },
        },
      ])
    ).toBe("2026-06-03T10:00:00.000Z");

    expect(nextAdminActionInvocationCursor(undefined)).toBeNull();
  });

  test("labels results", () => {
    expect(adminActionResultLabel(actions[0]!)).toBe("success");
    expect(adminActionResultLabel(actions[1]!)).toBe("failed");
  });

  test("chooses compact primary summaries for table rows and banners", () => {
    expect(adminActionPrimarySummary(actions[0]!)).toBe("ok");
    expect(adminActionPrimarySummary(actions[1]!)).toBe("window too wide");
    expect(
      adminActionPrimarySummary(
        adminAction({
          error_code: "permission_denied",
          error_message: null,
          success: false,
        })
      )
    ).toBe("permission_denied");
  });

  test("builds inspector details with action context and lineage", () => {
    const details = adminActionInspectorDetails(actions[1]!);

    expect(details.actionRows).toContainEqual(["module", "identity"]);
    expect(details.actionRows).toContainEqual(["action", "rebuild_index"]);
    expect(details.actionRows).toContainEqual([
      "capability",
      "identity.users.maintain",
    ]);
    expect(details.lineageRows).toContainEqual(["story_node", "admin_b"]);
    expect(details.lineageRows).toContainEqual(["correlation", "corr_admin"]);
    expect(details.summaries).toEqual({
      input_summary: "dry_run: true",
      result_summary: "ok",
    });
    expect(details.failure).toEqual({
      error_code: "action_validation_failed",
      error_message: "window too wide",
    });
  });

  test("builds operation paths", () => {
    expect(adminActionsPath()).toBe("/operations/admin-actions");
    expect(
      adminActionsPath({
        actionName: "sync_contacts",
        capability: "remote_crm.contacts.sync",
        correlationId: "corr_1",
        moduleName: "remote-crm",
        query: "contact",
        result: "failed",
        selectedId: "admin_1",
      })
    ).toBe(
      "/operations/admin-actions?action=sync_contacts&capability=remote_crm.contacts.sync&correlation_id=corr_1&module=remote-crm&q=contact&result=failed&selected=admin_1"
    );
  });
});

function adminAction(
  overrides: Partial<RuntimeAdminActionInvocation>
): RuntimeAdminActionInvocation {
  return {
    action_name: "sync_contacts",
    capability: "remote_crm.contacts.sync",
    correlation_id: "corr_admin",
    duration_ms: 1,
    error_code: null,
    error_message: null,
    id: "admin_test",
    input_summary: "dry_run: true",
    label: "Sync contacts",
    module_name: "remote-test",
    occurred_at: "2026-06-03T00:00:00.000Z",
    request_id: "req_admin",
    result_summary: "ok",
    span_id: null,
    success: true,
    trace_id: null,
    ...overrides,
  };
}
