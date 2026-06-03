import { useQuery } from "@tanstack/react-query";
import { useState } from "react";

import { cn } from "../lib/cn";
import { httpClient, isApiMode } from "../lib/http-client";
import {
  type AdminRecord,
  detailRows,
  type EntitySchema,
  moduleSourceHint,
  type ModuleSchema,
  recordId,
  renderRow,
} from "./data-render-model";

type SchemaResponse = { modules: ModuleSchema[] };
type ListResponse = {
  data: AdminRecord[];
  page: { limit: number; next_cursor: string | null };
};
type DetailResponse = { data: AdminRecord };

type Selection = { module: string; entity: EntitySchema };

const dataKeys = {
  schema: ["admin-data", "schema"] as const,
  list: (m: string, e: string) => ["admin-data", "list", m, e] as const,
  detail: (m: string, e: string, id: string) =>
    ["admin-data", "detail", m, e, id] as const,
};

export function DataPage() {
  const [selected, setSelected] = useState<Selection | null>(null);
  const [selectedRecordId, setSelectedRecordId] = useState<string | null>(null);

  const schemaQuery = useQuery({
    queryKey: dataKeys.schema,
    queryFn: () => httpClient.get("admin/data/schema").json<SchemaResponse>(),
    enabled: isApiMode(),
  });

  const listQuery = useQuery({
    queryKey: selected
      ? dataKeys.list(selected.module, selected.entity.name)
      : ["admin-data", "list", "none"],
    queryFn: () => {
      if (!selected) {
        throw new Error("no entity selected");
      }
      return httpClient
        .get(
          `admin/data/${encodeURIComponent(selected.module)}/${encodeURIComponent(selected.entity.name)}?limit=50`
        )
        .json<ListResponse>();
    },
    enabled: isApiMode() && selected !== null,
  });

  const detailQuery = useQuery({
    queryKey:
      selected && selectedRecordId
        ? dataKeys.detail(
            selected.module,
            selected.entity.name,
            selectedRecordId
          )
        : ["admin-data", "detail", "none"],
    queryFn: () => {
      if (!(selected && selectedRecordId)) {
        throw new Error("no record selected");
      }
      return httpClient
        .get(
          `admin/data/${encodeURIComponent(selected.module)}/${encodeURIComponent(selected.entity.name)}/${encodeURIComponent(selectedRecordId)}`
        )
        .json<DetailResponse>();
    },
    enabled: isApiMode() && selected !== null && selectedRecordId !== null,
  });

  if (!isApiMode()) {
    return <DataPlaceholder reason="schema-admin requires API mode" />;
  }

  return (
    <section className="grid h-full min-h-0 grid-rows-[auto_minmax(0,1fr)] overflow-hidden bg-(--background) text-(--foreground)">
      <header className="border-b border-(--border-subtle) bg-(--surface) px-3 py-2">
        <h1 className="font-mono text-[13px] font-semibold">Data</h1>
      </header>
      <div className="grid min-h-0 grid-cols-[220px_minmax(0,1fr)_320px]">
        <nav className="overflow-auto border-r border-(--border-subtle) p-2 font-mono text-[12px]">
          {schemaQuery.isError ? (
            <p className="px-2 py-1 text-(--muted)">Failed to load schema.</p>
          ) : schemaQuery.isPending ? (
            <p className="px-2 py-1 text-(--muted)">Loading…</p>
          ) : (
            schemaQuery.data?.modules.flatMap((moduleSchema) =>
              moduleSchema.schema.entities.map((entity) => {
                const isSelected =
                  selected !== null &&
                  selected.module === moduleSchema.module_name &&
                  selected.entity.name === entity.name;
                return (
                  <button
                    className={cn(
                      "block w-full px-2 py-1 text-left",
                      isSelected
                        ? "bg-(--accent-soft) shadow-[inset_2px_0_0_var(--accent)]"
                        : "hover:bg-(--sidebar)"
                    )}
                    key={`${moduleSchema.module_name}.${entity.name}`}
                    onClick={() => {
                      setSelected({ module: moduleSchema.module_name, entity });
                      setSelectedRecordId(null);
                    }}
                    type="button"
                  >
                    <span className="block truncate">
                      {moduleSchema.module_name} / {entity.label}
                    </span>
                    <span className="text-[10px] text-(--muted)">
                      {moduleSourceHint(moduleSchema.module_name)}
                    </span>
                  </button>
                );
              })
            )
          )}
        </nav>
        <div className="min-w-0 overflow-auto p-3 font-mono text-[12px]">
          {selected ? (
            listQuery.isError ? (
              <p className="text-(--muted)">
                Failed to load records: {String(listQuery.error.message)}
              </p>
            ) : listQuery.isPending ? (
              <p className="text-(--muted)">Loading…</p>
            ) : listQuery.data ? (
              <>
                <div className="mb-2 flex items-center gap-2 text-[11px] text-(--muted)">
                  <span>{selected.module}</span>
                  <span>/</span>
                  <span>{selected.entity.name}</span>
                  <span className="ml-auto border border-(--border-subtle) px-2 py-0.5 text-[10px] text-(--secondary)">
                    {moduleSourceHint(selected.module)}
                  </span>
                </div>
                <table className="w-full table-fixed">
                  <thead>
                    <tr>
                      {selected.entity.fields.map((field) => (
                        <th
                          className="px-2 py-1 text-left text-(--muted)"
                          key={field.name}
                        >
                          {field.label}
                        </th>
                      ))}
                    </tr>
                  </thead>
                  <tbody>
                    {listQuery.data.data.map((record, index) => {
                      const id = recordId(record);
                      const isSelected = id !== null && id === selectedRecordId;
                      return (
                        <tr
                          className={cn(
                            "border-t border-(--border-subtle)",
                            isSelected && "bg-(--accent-soft)"
                          )}
                          key={id ?? index}
                        >
                          {renderRow(selected.entity, record).map((cell) => (
                            <td className="p-0" key={cell.field}>
                              <button
                                className="block min-h-7 w-full truncate px-2 py-1 text-left disabled:cursor-default disabled:text-(--muted)"
                                disabled={id === null}
                                onClick={() => setSelectedRecordId(id)}
                                type="button"
                              >
                                {cell.display}
                              </button>
                            </td>
                          ))}
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              </>
            ) : null
          ) : (
            <p className="text-(--muted)">Select an entity.</p>
          )}
        </div>
        <aside className="min-w-0 overflow-auto border-l border-(--border-subtle) bg-(--surface) font-mono text-[12px]">
          <div className="border-b border-(--border-subtle) px-3 py-2">
            <h2 className="font-semibold">Detail</h2>
            <p className="mt-1 truncate text-[11px] text-(--muted)">
              {selected && selectedRecordId
                ? `${selected.module}/${selected.entity.name}/${selectedRecordId}`
                : "select a row"}
            </p>
          </div>
          <div className="p-3">
            {selected && selectedRecordId ? (
              detailQuery.isError ? (
                <p className="text-(--muted)">
                  Failed to load detail: {String(detailQuery.error.message)}
                </p>
              ) : detailQuery.isPending ? (
                <p className="text-(--muted)">Loading…</p>
              ) : detailQuery.data ? (
                <dl className="grid grid-cols-[96px_minmax(0,1fr)] border-y border-(--border-subtle)">
                  {detailRows(selected.entity, detailQuery.data.data).map(
                    (row) => (
                      <div className="contents" key={row.field}>
                        <dt className="border-b border-(--border-subtle) bg-(--sidebar) px-2 py-1.5 text-(--muted)">
                          {row.label}
                        </dt>
                        <dd className="min-w-0 truncate border-b border-(--border-subtle) px-2 py-1.5 text-(--secondary)">
                          {row.display}
                        </dd>
                      </div>
                    )
                  )}
                </dl>
              ) : null
            ) : (
              <p className="text-(--muted)">No record selected.</p>
            )}
          </div>
        </aside>
      </div>
    </section>
  );
}

function DataPlaceholder({ reason }: { reason: string }) {
  return (
    <section className="grid h-full place-items-center bg-(--background) font-mono text-[12px] text-(--muted)">
      {reason}
    </section>
  );
}
