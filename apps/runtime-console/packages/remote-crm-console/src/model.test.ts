import { describe, expect, test } from "vitest";

import { remoteCrmContactRows, remoteCrmContactsSummary } from "./model";

describe("remote crm console model", () => {
  test("formats remote contacts from admin data", () => {
    const contacts = [
      {
        active: true,
        company: "Analytical Engines Ltd",
        email: "ada@example.com",
        id: "contact_1",
        name: "Ada Lovelace",
      },
      {
        active: false,
        company: "Orbital Mechanics Co",
        email: "katherine@example.com",
        id: "contact_3",
        name: "Katherine Johnson",
      },
    ];

    expect(remoteCrmContactRows(contacts)).toEqual([
      {
        company: "Analytical Engines Ltd",
        email: "ada@example.com",
        id: "contact_1",
        name: "Ada Lovelace",
        status: "active",
      },
      {
        company: "Orbital Mechanics Co",
        email: "katherine@example.com",
        id: "contact_3",
        name: "Katherine Johnson",
        status: "paused",
      },
    ]);
    expect(remoteCrmContactsSummary(contacts)).toEqual({
      active: 1,
      paused: 1,
      total: 2,
    });
  });
});
