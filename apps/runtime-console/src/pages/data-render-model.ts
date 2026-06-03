// Mirrors platform-module's AdminSchema/FieldType JSON shapes. Hand-typed
// because the records are generic (serde_json::Value) — there is no per-entity
// generated SDK type.

export type FieldType =
  | { kind: "string" }
  | { kind: "integer" }
  | { kind: "boolean" }
  | { kind: "timestamp" }
  | { kind: "json" };

export type FieldSchema = {
  name: string;
  label: string;
  field_type: FieldType;
  nullable: boolean;
};

export type EntitySchema = {
  name: string;
  label: string;
  fields: FieldSchema[];
  read_capability: string;
};

export type AdminSchema = { entities: EntitySchema[] };

export type ModuleSchema = { module_name: string; schema: AdminSchema };

export type AdminRecord = Record<string, unknown>;

export type RenderedCell = {
  field: string;
  kind: FieldType["kind"];
  /** Display string for the value, already formatted per field type. */
  display: string;
};

/** Format one raw value per its field type into a display string. */
export function renderCell(field: FieldSchema, value: unknown): RenderedCell {
  const { kind } = field.field_type;
  let display: string;
  if (value === null || value === undefined) {
    display = "—";
  } else {
    switch (kind) {
      case "timestamp": {
        display = formatTimestamp(value);
        break;
      }
      case "boolean": {
        display = value ? "✓" : "✗";
        break;
      }
      case "json": {
        display = JSON.stringify(value);
        break;
      }
      default: {
        display = String(value);
      }
    }
  }
  return { field: field.name, kind, display };
}

/** Build the ordered cells for one record, driven by the entity's field schema. */
export function renderRow(
  entity: EntitySchema,
  record: AdminRecord
): RenderedCell[] {
  return entity.fields.map((field) => renderCell(field, record[field.name]));
}

function formatTimestamp(value: unknown): string {
  const date = new Date(String(value));
  return Number.isNaN(date.getTime()) ? String(value) : date.toISOString();
}
