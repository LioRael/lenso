import type { ConsoleAdminRecord } from "@lenso/runtime-console-api";

export type IdentityUserRow = {
  id: string;
  email: string;
  displayName: string;
  createdAt: string;
  updatedAt: string;
};

export type IdentityUsersSummary = {
  total: number;
  latestCreatedAt: string;
};

export function identityUserRows(
  records: readonly ConsoleAdminRecord[]
): IdentityUserRow[] {
  return records.map((record) => ({
    createdAt: fieldText(record.created_at),
    displayName: nullableFieldText(record.display_name),
    email: fieldText(record.email),
    id: fieldText(record.id),
    updatedAt: fieldText(record.updated_at),
  }));
}

export function identityUsersSummary(
  records: readonly ConsoleAdminRecord[]
): IdentityUsersSummary {
  const rows = identityUserRows(records);
  return {
    latestCreatedAt: rows[0]?.createdAt ?? "-",
    total: rows.length,
  };
}

function fieldText(value: unknown): string {
  return typeof value === "string" && value.length > 0 ? value : "-";
}

function nullableFieldText(value: unknown): string {
  return value === null || value === undefined ? "-" : fieldText(value);
}
