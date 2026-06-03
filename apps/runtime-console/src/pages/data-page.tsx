import { useQuery } from "@tanstack/react-query";
import { useState } from "react";

import { cn } from "../lib/cn";
import { httpClient, isApiMode } from "../lib/http-client";
import {
  type AdminRecord,
  type EntitySchema,
  type ModuleSchema,
  renderRow,
} from "./data-render-model";

type SchemaResponse = { modules: ModuleSchema[] };
type ListResponse = {
  data: AdminRecord[];
  page: { limit: number; next_cursor: string | null };
};

type Selection = { module: string; entity: EntitySchema };

const dataKeys = {
  schema: ["admin-data", "schema"] as const,
  list: (m: string, e: string) => ["admin-data", "list", m, e] as const,
};

export function DataPage() {
  const [selected, setSelected] = useState<Selection | null>(null);

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

  if (!isApiMode()) {
    return <DataPlaceholder reason="schema-admin requires API mode" />;
  }

  return (
    <section className="grid h-full min-h-0 grid-rows-[auto_minmax(0,1fr)] overflow-hidden bg-(--background) text-(--foreground)">
      <header className="border-b border-(--border-subtle) bg-(--surface) px-3 py-2">
        <h1 className="font-mono text-[13px] font-semibold">Data</h1>
      </header>
      <div className="grid min-h-0 grid-cols-[220px_minmax(0,1fr)]">
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
                    onClick={() =>
                      setSelected({ module: moduleSchema.module_name, entity })
                    }
                    type="button"
                  >
                    {moduleSchema.module_name} / {entity.label}
                  </button>
                );
              })
            )
          )}
        </nav>
        <div className="overflow-auto p-3 font-mono text-[12px]">
          {selected ? (
            listQuery.isError ? (
              <p className="text-(--muted)">Failed to load records.</p>
            ) : listQuery.isPending ? (
              <p className="text-(--muted)">Loading…</p>
            ) : listQuery.data ? (
              <table className="w-full">
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
                  {listQuery.data.data.map((record, index) => (
                    <tr
                      className="border-t border-(--border-subtle)"
                      key={index}
                    >
                      {renderRow(selected.entity, record).map((cell) => (
                        <td className="px-2 py-1" key={cell.field}>
                          {cell.display}
                        </td>
                      ))}
                    </tr>
                  ))}
                </tbody>
              </table>
            ) : null
          ) : (
            <p className="text-(--muted)">Select an entity.</p>
          )}
        </div>
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
