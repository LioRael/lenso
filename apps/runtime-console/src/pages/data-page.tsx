import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { AlertTriangle, Code2, RefreshCw } from "lucide-react";
import { useState } from "react";

import { Button } from "../components/ui/button";
import { cn } from "../lib/cn";
import { httpClient, isApiMode } from "../lib/http-client";
import {
  type AdminModuleMetadata,
  type AdminRecord,
  adminSurfaceLabel,
  adminSurfaceMetadataRows,
  type DeclarativeComponent,
  declarativeEntitySection,
  declarativeMetricValues,
  detailRows,
  embeddedIframePolicy,
  type EntitySchema,
  firstDeclarativePage,
  moduleErrorMessage,
  moduleIsLoaded,
  moduleNavItems,
  moduleStatusLabel,
  recordId,
  renderRow,
} from "./data-render-model";

type ModulesResponse = { modules: AdminModuleMetadata[] };
type ListResponse = {
  data: AdminRecord[];
  page: { limit: number; next_cursor: string | null };
};
type DetailResponse = { data: AdminRecord };

type Selection = { module: AdminModuleMetadata; entity: EntitySchema | null };

const dataKeys = {
  modules: ["admin-data", "modules"] as const,
  list: (m: string, e: string) => ["admin-data", "list", m, e] as const,
  detail: (m: string, e: string, id: string) =>
    ["admin-data", "detail", m, e, id] as const,
};

export function DataPage() {
  const queryClient = useQueryClient();
  const [selected, setSelected] = useState<Selection | null>(null);
  const [selectedRecordId, setSelectedRecordId] = useState<string | null>(null);

  const modulesQuery = useQuery({
    queryKey: dataKeys.modules,
    queryFn: () => httpClient.get("admin/data/modules").json<ModulesResponse>(),
    enabled: isApiMode(),
  });

  const listQuery = useQuery({
    queryKey: selected
      ? dataKeys.list(
          selected.module.module_name,
          selected.entity?.name ?? "module"
        )
      : ["admin-data", "list", "none"],
    queryFn: () => {
      if (!(selected && selected.entity)) {
        throw new Error("no entity selected");
      }
      return httpClient
        .get(
          `admin/data/${encodeURIComponent(selected.module.module_name)}/${encodeURIComponent(selected.entity.name)}?limit=50`
        )
        .json<ListResponse>();
    },
    enabled:
      isApiMode() &&
      selected !== null &&
      selected.entity !== null &&
      moduleIsLoaded(selected.module),
  });

  const detailQuery = useQuery({
    queryKey:
      selected && selected.entity && selectedRecordId
        ? dataKeys.detail(
            selected.module.module_name,
            selected.entity.name,
            selectedRecordId
          )
        : ["admin-data", "detail", "none"],
    queryFn: () => {
      if (!(selected && selected.entity && selectedRecordId)) {
        throw new Error("no record selected");
      }
      return httpClient
        .get(
          `admin/data/${encodeURIComponent(selected.module.module_name)}/${encodeURIComponent(selected.entity.name)}/${encodeURIComponent(selectedRecordId)}`
        )
        .json<DetailResponse>();
    },
    enabled:
      isApiMode() &&
      selected !== null &&
      selected.entity !== null &&
      moduleIsLoaded(selected.module) &&
      selectedRecordId !== null,
  });

  const refreshMutation = useMutation({
    mutationFn: () =>
      httpClient.post("admin/data/schema/refresh").json<unknown>(),
    onSuccess: async () => {
      setSelected(null);
      setSelectedRecordId(null);
      await queryClient.invalidateQueries({ queryKey: dataKeys.modules });
      await queryClient.invalidateQueries({ queryKey: ["admin-data", "list"] });
      await queryClient.invalidateQueries({
        queryKey: ["admin-data", "detail"],
      });
    },
  });

  if (!isApiMode()) {
    return <DataPlaceholder reason="schema-admin requires API mode" />;
  }

  return (
    <section className="grid h-full min-h-0 grid-rows-[auto_minmax(0,1fr)] overflow-hidden bg-(--background) text-(--foreground)">
      <header className="flex items-center border-b border-(--border-subtle) bg-(--surface) px-3 py-2">
        <h1 className="font-mono text-[13px] font-semibold">Data</h1>
        <Button
          aria-label="Refresh admin modules"
          className="ml-auto min-h-6 px-2"
          disabled={refreshMutation.isPending}
          onClick={() => refreshMutation.mutate()}
          title="Refresh admin modules"
          type="button"
          variant="ghost"
        >
          <RefreshCw
            className={cn(refreshMutation.isPending && "animate-spin")}
            size={13}
          />
          Refresh
        </Button>
      </header>
      <div className="grid min-h-0 grid-cols-[220px_minmax(0,1fr)_320px]">
        <nav className="overflow-auto border-r border-(--border-subtle) p-2 font-mono text-[12px]">
          {modulesQuery.isError ? (
            <p className="px-2 py-1 text-(--muted)">Failed to load modules.</p>
          ) : modulesQuery.isPending ? (
            <p className="px-2 py-1 text-(--muted)">Loading…</p>
          ) : modulesQuery.data ? (
            moduleNavItems(modulesQuery.data.modules).map((item) => {
              const isSelected =
                selected !== null &&
                selected.module.module_name === item.module.module_name &&
                selected.entity?.name === item.entity?.name;
              return (
                <button
                  className={cn(
                    "block w-full px-2 py-1 text-left disabled:cursor-default",
                    isSelected
                      ? "bg-(--accent-soft) shadow-[inset_2px_0_0_var(--accent)]"
                      : "hover:bg-(--sidebar)",
                    moduleIsLoaded(item.module)
                      ? null
                      : "border-l border-[color-mix(in_srgb,var(--error)_45%,transparent)] text-(--secondary)"
                  )}
                  key={item.key}
                  onClick={() => {
                    setSelected({ module: item.module, entity: item.entity });
                    setSelectedRecordId(null);
                  }}
                  type="button"
                >
                  <span className="flex min-w-0 items-center gap-1.5">
                    {moduleIsLoaded(item.module) ? null : (
                      <AlertTriangle
                        className="shrink-0 text-(--error)"
                        size={12}
                      />
                    )}
                    <span className="truncate">{item.label}</span>
                  </span>
                  <span className="block truncate text-[10px] text-(--muted)">
                    {item.sublabel}
                  </span>
                </button>
              );
            })
          ) : null}
          {refreshMutation.isError ? (
            <p className="px-2 py-2 text-[11px] text-(--error)">
              Refresh failed: {String(refreshMutation.error.message)}
            </p>
          ) : null}
        </nav>
        <div className="min-w-0 overflow-auto p-3 font-mono text-[12px]">
          {selected && !moduleIsLoaded(selected.module) ? (
            <ModuleErrorPanel module={selected.module} />
          ) : selected && selected.entity ? (
            listQuery.isError ? (
              <p className="text-(--muted)">
                Failed to load records: {String(listQuery.error.message)}
              </p>
            ) : listQuery.isPending ? (
              <p className="text-(--muted)">Loading…</p>
            ) : listQuery.data ? (
              <RecordsTable
                entity={selected.entity}
                module={selected.module}
                records={listQuery.data.data}
                selectedRecordId={selectedRecordId}
                setSelectedRecordId={setSelectedRecordId}
              />
            ) : null
          ) : selected ? (
            <ModuleSurfacePanel module={selected.module} />
          ) : (
            <p className="text-(--muted)">Select a module or entity.</p>
          )}
        </div>
        <aside className="min-w-0 overflow-auto border-l border-(--border-subtle) bg-(--surface) font-mono text-[12px]">
          <div className="border-b border-(--border-subtle) px-3 py-2">
            <h2 className="font-semibold">Detail</h2>
            <p className="mt-1 truncate text-[11px] text-(--muted)">
              {selected && selected.entity && selectedRecordId
                ? `${selected.module.module_name}/${selected.entity.name}/${selectedRecordId}`
                : selected && !moduleIsLoaded(selected.module)
                  ? `${selected.module.module_name} unavailable`
                  : selected
                    ? `${selected.module.module_name} surface`
                    : "select a row"}
            </p>
          </div>
          <div className="p-3">
            {selected && !moduleIsLoaded(selected.module) ? (
              <ModuleErrorPanel module={selected.module} compact />
            ) : selected && selected.entity && selectedRecordId ? (
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
            ) : selected ? (
              <ModuleSurfacePanel compact module={selected.module} />
            ) : (
              <p className="text-(--muted)">No record selected.</p>
            )}
          </div>
        </aside>
      </div>
    </section>
  );
}

function RecordsTable({
  entity,
  module,
  records,
  selectedRecordId,
  setSelectedRecordId,
}: {
  entity: EntitySchema;
  module: AdminModuleMetadata;
  records: AdminRecord[];
  selectedRecordId: string | null;
  setSelectedRecordId: (id: string | null) => void;
}) {
  return (
    <>
      <div className="mb-2 flex items-center gap-2 text-[11px] text-(--muted)">
        <span>{module.module_name}</span>
        <span>/</span>
        <span>{entity.name}</span>
        <span className="ml-auto border border-(--border-subtle) px-2 py-0.5 text-[10px] text-(--secondary)">
          {module.source} / {moduleStatusLabel(module)}
        </span>
      </div>
      <table className="w-full table-fixed">
        <thead>
          <tr>
            {entity.fields.map((field) => (
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
          {records.map((record, index) => {
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
                {renderRow(entity, record).map((cell) => (
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
  );
}

function ModuleErrorPanel({
  compact = false,
  module,
}: {
  compact?: boolean;
  module: AdminModuleMetadata;
}) {
  return (
    <div
      className={cn(
        "border border-[color-mix(in_srgb,var(--error)_35%,var(--border-subtle))] bg-[color-mix(in_srgb,var(--error)_8%,transparent)] p-3",
        compact && "text-[11px]"
      )}
    >
      <div className="flex items-center gap-2 font-semibold text-(--foreground)">
        <AlertTriangle className="text-(--error)" size={14} />
        <span>{module.module_name}</span>
        <span className="ml-auto border border-(--border-subtle) px-2 py-0.5 text-[10px] text-(--secondary)">
          {module.source} / {moduleStatusLabel(module)}
        </span>
      </div>
      <p className="mt-2 break-words text-(--muted)">
        {moduleErrorMessage(module)}
      </p>
    </div>
  );
}

function ModuleSurfacePanel({
  compact = false,
  module,
}: {
  compact?: boolean;
  module: AdminModuleMetadata;
}) {
  const rows = adminSurfaceMetadataRows(module);
  const surfaceLabel = adminSurfaceLabel(module.admin);
  const iframePolicy = embeddedIframePolicy(module.admin);
  return (
    <div
      className={cn(
        "grid gap-3 border border-(--border-subtle) bg-(--surface) p-3",
        compact && "text-[11px]"
      )}
    >
      <div className="flex items-center gap-2 font-semibold text-(--foreground)">
        <Code2 className="text-(--info)" size={14} />
        <span>{module.module_name}</span>
        <span className="ml-auto border border-(--border-subtle) px-2 py-0.5 text-[10px] text-(--secondary)">
          {surfaceLabel}
        </span>
      </div>
      {module.admin?.kind === "embedded_custom" ? (
        <EmbeddedIframeSurface compact={compact} policy={iframePolicy} />
      ) : module.admin?.kind === "declarative_custom" ? (
        <DeclarativeSurface
          compact={compact}
          module={module}
          surface={module.admin}
        />
      ) : (
        <p className="text-(--muted)">
          {module.admin?.kind === "schema"
            ? "Schema surface has no selectable entity."
            : "Custom admin surface is discoverable. Rendering is waiting for a host renderer."}
        </p>
      )}
      <dl className="grid grid-cols-[120px_minmax(0,1fr)] border-y border-(--border-subtle)">
        {rows.map((row) => (
          <div className="contents" key={row.label}>
            <dt className="border-b border-(--border-subtle) bg-(--sidebar) px-2 py-1.5 text-(--muted)">
              {row.label}
            </dt>
            <dd className="min-w-0 truncate border-b border-(--border-subtle) px-2 py-1.5 text-(--secondary)">
              {row.value}
            </dd>
          </div>
        ))}
      </dl>
    </div>
  );
}

function DeclarativeSurface({
  compact,
  module,
  surface,
}: {
  compact: boolean;
  module: AdminModuleMetadata;
  surface: Extract<
    AdminModuleMetadata["admin"],
    { kind: "declarative_custom" }
  >;
}) {
  const page = firstDeclarativePage(surface);
  if (!page) {
    return <p className="text-(--muted)">No declarative pages declared.</p>;
  }

  return (
    <div className="grid gap-3">
      <div className="flex items-center gap-2 text-[11px] text-(--muted)">
        <span>{page.label}</span>
        <span className="ml-auto border border-(--border-subtle) px-2 py-0.5 text-[10px] text-(--secondary)">
          host rendered
        </span>
      </div>
      {(page.sections ?? []).map((section) => (
        <section
          className="border border-(--border-subtle) bg-(--background)"
          key={section.name}
        >
          <header className="border-b border-(--border-subtle) px-2 py-1.5 font-semibold">
            {section.label}
          </header>
          <div className={cn("p-2", compact && "text-[11px]")}>
            <DeclarativeComponentView
              component={section.component}
              module={module}
              surface={surface}
            />
          </div>
        </section>
      ))}
    </div>
  );
}

function DeclarativeComponentView({
  component,
  module,
  surface,
}: {
  component: DeclarativeComponent;
  module: AdminModuleMetadata;
  surface: Extract<
    AdminModuleMetadata["admin"],
    { kind: "declarative_custom" }
  >;
}) {
  switch (component.kind) {
    case "metric_strip": {
      const metrics = declarativeMetricValues(surface, component.metrics ?? []);
      return (
        <div className="grid grid-cols-[repeat(auto-fit,minmax(120px,1fr))] gap-2">
          {metrics.map((metric) => (
            <div
              className="border border-(--border-subtle) bg-(--surface) px-2 py-1.5"
              key={metric.label}
            >
              <div className="truncate text-[10px] text-(--muted)">
                {metric.label}
              </div>
              <div className="mt-1 truncate text-(--foreground)">
                {metric.value}
              </div>
            </div>
          ))}
        </div>
      );
    }
    case "entity_table": {
      const { entity, reason } = declarativeEntitySection(
        surface,
        component.entity
      );
      if (!entity) {
        return <p className="text-(--muted)">{reason}</p>;
      }
      return <DeclarativeEntityTable entity={entity} module={module} />;
    }
    case "entity_detail": {
      const { entity, reason } = declarativeEntitySection(
        surface,
        component.entity
      );
      return (
        <p className="text-(--muted)">
          {entity
            ? `Entity detail renderer is not enabled for ${entity.label}.`
            : reason}
        </p>
      );
    }
    default: {
      return (
        <p className="text-(--muted)">Unsupported declarative component.</p>
      );
    }
  }
}

function DeclarativeEntityTable({
  entity,
  module,
}: {
  entity: EntitySchema;
  module: AdminModuleMetadata;
}) {
  const recordsQuery = useQuery({
    queryKey: dataKeys.list(module.module_name, entity.name),
    queryFn: () =>
      httpClient
        .get(
          `admin/data/${encodeURIComponent(module.module_name)}/${encodeURIComponent(entity.name)}?limit=50`
        )
        .json<ListResponse>(),
    enabled: isApiMode() && moduleIsLoaded(module),
  });

  if (recordsQuery.isError) {
    return (
      <p className="text-(--muted)">
        Failed to load records: {String(recordsQuery.error.message)}
      </p>
    );
  }
  if (recordsQuery.isPending) {
    return <p className="text-(--muted)">Loading…</p>;
  }

  return (
    <RecordsTable
      entity={entity}
      module={module}
      records={recordsQuery.data.data}
      selectedRecordId={null}
      setSelectedRecordId={() => undefined}
    />
  );
}

function EmbeddedIframeSurface({
  compact,
  policy,
}: {
  compact: boolean;
  policy: ReturnType<typeof embeddedIframePolicy>;
}) {
  if (policy.status === "blocked") {
    return (
      <div className="border border-[color-mix(in_srgb,var(--warning)_35%,var(--border-subtle))] bg-[color-mix(in_srgb,var(--warning)_8%,transparent)] p-2">
        <div className="flex items-center gap-2 font-semibold text-(--foreground)">
          <AlertTriangle className="text-(--warning)" size={14} />
          <span>iframe blocked</span>
        </div>
        <p className="mt-2 break-words text-(--muted)">{policy.reason}</p>
      </div>
    );
  }

  return (
    <div className="grid gap-2">
      <div className="flex items-center gap-2 text-[11px] text-(--muted)">
        <span>iframe</span>
        <span className="truncate">{policy.origin}</span>
        <span className="ml-auto border border-(--border-subtle) px-2 py-0.5 text-[10px] text-(--secondary)">
          no bridge
        </span>
      </div>
      <iframe
        className={cn(
          "w-full border border-(--border-subtle) bg-(--background)",
          compact ? "h-44" : "h-[min(520px,calc(100vh-230px))]"
        )}
        referrerPolicy="no-referrer"
        sandbox={policy.sandbox}
        src={policy.url}
        title="Embedded admin surface"
      />
    </div>
  );
}

function DataPlaceholder({ reason }: { reason: string }) {
  return (
    <section className="grid h-full place-items-center bg-(--background) font-mono text-[12px] text-(--muted)">
      {reason}
    </section>
  );
}
