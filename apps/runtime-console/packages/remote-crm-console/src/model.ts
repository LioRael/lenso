import type { ConsoleAdminRecord } from "@lenso/runtime-console-api";

export interface RemoteCrmContactRow {
  id: string;
  name: string;
  email: string;
  company: string;
  status: "active" | "paused";
}

export interface RemoteCrmContactsSummary {
  active: number;
  paused: number;
  total: number;
}

const fieldText = (value: unknown): string =>
  typeof value === "string" && value.length > 0 ? value : "-";

export const remoteCrmContactRows = (
  records: readonly ConsoleAdminRecord[]
): RemoteCrmContactRow[] =>
  records.map((record) => ({
    company: fieldText(record.company),
    email: fieldText(record.email),
    id: fieldText(record.id),
    name: fieldText(record.name),
    status: record.active === false ? "paused" : "active",
  }));

export const remoteCrmContactsSummary = (
  records: readonly ConsoleAdminRecord[]
): RemoteCrmContactsSummary => {
  const summary: RemoteCrmContactsSummary = { active: 0, paused: 0, total: 0 };
  for (const row of remoteCrmContactRows(records)) {
    summary.total += 1;
    summary[row.status] += 1;
  }
  return summary;
};
