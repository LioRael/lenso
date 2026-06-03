import { describe, expect, test } from "vitest";

import {
  type EntitySchema,
  type FieldSchema,
  renderCell,
  renderRow,
} from "./data-render-model";

const emailField: FieldSchema = {
  name: "email",
  label: "Email",
  field_type: { kind: "string" },
  nullable: false,
};
const activeField: FieldSchema = {
  name: "active",
  label: "Active",
  field_type: { kind: "boolean" },
  nullable: false,
};
const createdAtField: FieldSchema = {
  name: "created_at",
  label: "Created",
  field_type: { kind: "timestamp" },
  nullable: false,
};
const metaField: FieldSchema = {
  name: "meta",
  label: "Meta",
  field_type: { kind: "json" },
  nullable: true,
};

const entity: EntitySchema = {
  name: "users",
  label: "Users",
  read_capability: "identity.users.read",
  fields: [emailField, activeField, createdAtField, metaField],
};

describe("renderCell", () => {
  test("renders strings verbatim", () => {
    expect(renderCell(emailField, "a@example.com").display).toBe(
      "a@example.com"
    );
  });
  test("renders booleans as check/cross", () => {
    expect(renderCell(activeField, true).display).toBe("✓");
    expect(renderCell(activeField, false).display).toBe("✗");
  });
  test("renders timestamps as ISO", () => {
    expect(renderCell(createdAtField, "2026-06-03T00:00:00Z").display).toBe(
      "2026-06-03T00:00:00.000Z"
    );
  });
  test("stringifies json", () => {
    expect(renderCell(metaField, { a: 1 }).display).toBe('{"a":1}');
  });
  test("renders null/absent as em dash", () => {
    expect(renderCell(emailField, null).display).toBe("—");
    expect(renderCell(metaField, undefined).display).toBe("—");
  });
  test("renders integers as string", () => {
    const intField: FieldSchema = {
      name: "count",
      label: "Count",
      field_type: { kind: "integer" },
      nullable: false,
    };
    expect(renderCell(intField, 42).display).toBe("42");
  });
});

describe("renderRow", () => {
  test("produces one cell per schema field, in order", () => {
    const cells = renderRow(entity, {
      email: "a@example.com",
      active: true,
      created_at: "2026-06-03T00:00:00Z",
      meta: { x: 1 },
    });
    expect(cells.map((c) => c.field)).toEqual([
      "email",
      "active",
      "created_at",
      "meta",
    ]);
  });
});
