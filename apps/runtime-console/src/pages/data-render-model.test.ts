import { describe, expect, test } from "vitest";

import {
  detailRows,
  type EntitySchema,
  type FieldSchema,
  moduleErrorMessage,
  moduleIsLoaded,
  moduleNavItems,
  type ModuleSchema,
  moduleStatusLabel,
  recordId,
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

describe("recordId", () => {
  test("uses a string id when present", () => {
    expect(recordId({ id: "contact_1", email: "a@example.com" })).toBe(
      "contact_1"
    );
  });

  test("returns null when the record has no string id", () => {
    expect(recordId({ email: "a@example.com" })).toBeNull();
    expect(recordId({ id: 42 })).toBeNull();
  });
});

describe("detailRows", () => {
  test("renders detail values from schema fields in order", () => {
    const rows = detailRows(entity, {
      email: "a@example.com",
      active: true,
      created_at: "2026-06-03T00:00:00Z",
      meta: { x: 1 },
    });

    expect(rows).toEqual([
      { field: "email", label: "Email", display: "a@example.com" },
      { field: "active", label: "Active", display: "✓" },
      {
        field: "created_at",
        label: "Created",
        display: "2026-06-03T00:00:00.000Z",
      },
      { field: "meta", label: "Meta", display: '{"x":1}' },
    ]);
  });
});

describe("moduleStatusLabel", () => {
  const moduleSchema: ModuleSchema = {
    module_name: "remote-crm",
    source: "remote",
    status: "loaded",
    error: null,
    schema: { entities: [] },
  };

  test("uses backend loaded status verbatim", () => {
    expect(moduleStatusLabel(moduleSchema)).toBe("loaded");
  });

  test("maps backend error status objects to error", () => {
    expect(
      moduleStatusLabel({
        ...moduleSchema,
        status: "error",
        error: "manifest failed",
      })
    ).toBe("error");
  });
});

describe("module status helpers", () => {
  const loadedModule: ModuleSchema = {
    module_name: "identity",
    source: "linked",
    status: "loaded",
    error: null,
    schema: { entities: [entity] },
  };
  const errorModule: ModuleSchema = {
    module_name: "remote-crm",
    source: "remote",
    status: "error",
    error: "remote manifest request failed",
    schema: { entities: [] },
  };

  test("identifies loaded modules", () => {
    expect(moduleIsLoaded(loadedModule)).toBe(true);
    expect(moduleIsLoaded(errorModule)).toBe(false);
  });

  test("returns the backend error message for failed modules", () => {
    expect(moduleErrorMessage(errorModule)).toBe(
      "remote manifest request failed"
    );
    expect(moduleErrorMessage(loadedModule)).toBeNull();
  });

  test("keeps failed empty-schema modules visible in nav", () => {
    expect(moduleNavItems([loadedModule, errorModule])).toEqual([
      {
        key: "identity.users",
        module: loadedModule,
        entity,
        label: "identity / Users",
        sublabel: "linked / loaded",
      },
      {
        key: "remote-crm",
        module: errorModule,
        entity: null,
        label: "remote-crm",
        sublabel: "remote / error",
      },
    ]);
  });
});
