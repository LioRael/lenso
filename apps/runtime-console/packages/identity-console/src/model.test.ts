import { describe, expect, test } from "vitest";

import { identityUserRows, identityUsersSummary } from "./model";

describe("identity console model", () => {
  test("formats identity user records from admin data", () => {
    const users = [
      {
        created_at: "2026-06-06T09:00:00.000Z",
        display_name: "Ada Lovelace",
        email: "ada@example.com",
        id: "usr_ada",
        updated_at: "2026-06-06T09:30:00.000Z",
      },
      {
        created_at: "2026-06-05T09:00:00.000Z",
        display_name: null,
        email: "grace@example.com",
        id: "usr_grace",
        updated_at: "2026-06-05T09:30:00.000Z",
      },
    ];

    expect(identityUserRows(users)).toEqual([
      {
        createdAt: "2026-06-06T09:00:00.000Z",
        displayName: "Ada Lovelace",
        email: "ada@example.com",
        id: "usr_ada",
        updatedAt: "2026-06-06T09:30:00.000Z",
      },
      {
        createdAt: "2026-06-05T09:00:00.000Z",
        displayName: "-",
        email: "grace@example.com",
        id: "usr_grace",
        updatedAt: "2026-06-05T09:30:00.000Z",
      },
    ]);
    expect(identityUsersSummary(users)).toEqual({
      latestCreatedAt: "2026-06-06T09:00:00.000Z",
      total: 2,
    });
  });
});
