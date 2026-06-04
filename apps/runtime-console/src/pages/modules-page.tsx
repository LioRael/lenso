import { useQuery } from "@tanstack/react-query";
import { Boxes, Route, TriangleAlert } from "lucide-react";
import { useState } from "react";

import { cn } from "../lib/cn";
import {
  httpClient,
  isApiMode,
  runtimeConsoleDataSource,
} from "../lib/http-client";
import {
  type AdminModuleMetadata,
  adminSurfaceLabel,
  adminSurfaceMetadataRows,
  moduleErrorMessage,
  moduleHttpRouteRows,
  moduleIsLoaded,
  moduleStatusLabel,
} from "./data-render-model";

type ModulesResponse = { modules: AdminModuleMetadata[] };

const modulesQueryKey = ["modules", "registry"] as const;
const emptyModules: AdminModuleMetadata[] = [];

export function ModulesPage() {
  if (!isApiMode()) {
    return <ModulesPlaceholder reason="modules registry requires API mode" />;
  }
  return <ModulesContent />;
}

function ModulesContent() {
  const modulesQuery = useQuery({
    enabled: isApiMode(),
    queryKey: modulesQueryKey,
    queryFn: () => httpClient.get("admin/data/modules").json<ModulesResponse>(),
  });
  const modules = modulesQuery.data?.modules ?? emptyModules;
  const [selectedModuleName, setSelectedModuleName] = useState<string | null>(
    null
  );
  const selectedModule =
    modules.find((module) => module.module_name === selectedModuleName) ??
    modules[0] ??
    null;

  return (
    <section className="grid h-full min-h-0 grid-rows-[auto_minmax(0,1fr)] overflow-hidden bg-(--background) text-(--foreground)">
      <header className="border-b border-(--border-subtle) bg-(--surface) px-3 py-2">
        <div className="flex items-center gap-2">
          <Boxes className="text-(--accent)" size={14} />
          <h1 className="font-mono text-[13px] font-semibold">Modules</h1>
          <span className="ml-auto font-mono text-[10px] text-(--muted)">
            {modules.length} modules / {runtimeConsoleDataSource()}
          </span>
        </div>
      </header>

      <div className="grid min-h-0 grid-cols-[260px_minmax(0,1fr)] overflow-hidden">
        <nav className="min-h-0 overflow-auto border-r border-(--border-subtle) p-2 font-mono text-[12px]">
          {modulesQuery.isLoading ? (
            <p className="px-2 py-1 text-(--muted)">Loading...</p>
          ) : modulesQuery.isError ? (
            <p className="px-2 py-1 text-(--error)">Failed to load modules.</p>
          ) : modules.length === 0 ? (
            <p className="px-2 py-1 text-(--muted)">No modules registered.</p>
          ) : (
            modules.map((module) => {
              const selected =
                selectedModule?.module_name === module.module_name;
              return (
                <button
                  className={cn(
                    "block w-full px-2 py-1 text-left",
                    selected
                      ? "bg-(--accent-soft) shadow-[inset_2px_0_0_var(--accent)]"
                      : "hover:bg-(--sidebar)",
                    moduleIsLoaded(module)
                      ? null
                      : "border-l border-[color-mix(in_srgb,var(--error)_45%,transparent)] text-(--secondary)"
                  )}
                  key={module.module_name}
                  onClick={() => setSelectedModuleName(module.module_name)}
                  type="button"
                >
                  <span className="flex min-w-0 items-center gap-1.5">
                    {moduleIsLoaded(module) ? null : (
                      <TriangleAlert
                        className="shrink-0 text-(--error)"
                        size={12}
                      />
                    )}
                    <span className="truncate">{module.module_name}</span>
                  </span>
                  <span className="block truncate text-[10px] text-(--muted)">
                    {module.source} / {adminSurfaceLabel(module.admin)} /{" "}
                    {moduleStatusLabel(module)}
                  </span>
                </button>
              );
            })
          )}
        </nav>

        <main className="min-h-0 overflow-auto p-3 font-mono text-[12px]">
          {selectedModule ? (
            <ModuleRegistryDetail module={selectedModule} />
          ) : (
            <p className="text-(--muted)">Select a module.</p>
          )}
        </main>
      </div>
    </section>
  );
}

function ModuleRegistryDetail({ module }: { module: AdminModuleMetadata }) {
  const routeRows = moduleHttpRouteRows(module);
  return (
    <div className="grid gap-3">
      <section className="border border-(--border-subtle) bg-(--surface)">
        <header className="flex items-center gap-2 border-b border-(--border-subtle) px-3 py-2 font-semibold">
          <Boxes className="text-(--info)" size={14} />
          <span>{module.module_name}</span>
          <span className="ml-auto border border-(--border-subtle) px-2 py-0.5 text-[10px] text-(--secondary)">
            {module.source} / {moduleStatusLabel(module)}
          </span>
        </header>
        {moduleIsLoaded(module) ? (
          <MetadataRows rows={adminSurfaceMetadataRows(module)} />
        ) : (
          <p className="px-3 py-2 text-(--error)">
            {moduleErrorMessage(module)}
          </p>
        )}
      </section>

      <ModuleHttpRoutesTable rows={routeRows} />
    </div>
  );
}

function MetadataRows({ rows }: { rows: { label: string; value: string }[] }) {
  return (
    <dl className="grid grid-cols-[150px_minmax(0,1fr)] border-b border-(--border-subtle)">
      {rows.map((row) => (
        <div className="contents" key={row.label}>
          <dt className="border-t border-(--border-subtle) bg-(--sidebar) px-3 py-1.5 text-(--muted)">
            {row.label}
          </dt>
          <dd className="min-w-0 truncate border-t border-(--border-subtle) px-3 py-1.5 text-(--secondary)">
            {row.value}
          </dd>
        </div>
      ))}
    </dl>
  );
}

function ModuleHttpRoutesTable({
  rows,
}: {
  rows: ReturnType<typeof moduleHttpRouteRows>;
}) {
  if (rows.length === 0) {
    return (
      <section className="border border-(--border-subtle) bg-(--surface) px-3 py-2 text-(--muted)">
        No HTTP interfaces declared.
      </section>
    );
  }

  return (
    <section className="min-w-0 border border-(--border-subtle) bg-(--surface)">
      <header className="flex items-center gap-2 border-b border-(--border-subtle) px-3 py-2 font-semibold">
        <Route className="text-(--accent)" size={14} />
        <span>HTTP Interfaces</span>
        <span className="ml-auto border border-(--border-subtle) px-1.5 py-0.5 text-[10px] text-(--secondary)">
          {rows.length}
        </span>
      </header>
      <div className="overflow-auto">
        <table className="w-full min-w-[760px] table-fixed">
          <thead className="bg-(--sidebar) text-[10px] uppercase tracking-wide text-(--muted)">
            <tr>
              <th className="w-16 px-3 py-1.5 text-left">method</th>
              <th className="px-3 py-1.5 text-left">path</th>
              <th className="px-3 py-1.5 text-left">display</th>
              <th className="px-3 py-1.5 text-left">story</th>
              <th className="px-3 py-1.5 text-left">capability</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((route) => (
              <tr
                className="border-t border-(--border-subtle) text-[11px]"
                key={route.key}
              >
                <td className="px-3 py-1.5 text-(--accent)">{route.method}</td>
                <td className="truncate px-3 py-1.5 text-(--foreground)">
                  {route.path}
                </td>
                <td className="truncate px-3 py-1.5 text-(--secondary)">
                  {route.displayName}
                </td>
                <td className="truncate px-3 py-1.5 text-(--secondary)">
                  {route.storyTitle}
                </td>
                <td className="truncate px-3 py-1.5 text-(--muted)">
                  {route.capability}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  );
}

function ModulesPlaceholder({ reason }: { reason: string }) {
  return (
    <section className="grid h-full min-h-0 grid-rows-[auto_minmax(0,1fr)] overflow-hidden bg-(--background) text-(--foreground)">
      <header className="border-b border-(--border-subtle) bg-(--surface) px-3 py-2">
        <div className="flex items-center gap-2">
          <Boxes className="text-(--accent)" size={14} />
          <h1 className="font-mono text-[13px] font-semibold">Modules</h1>
        </div>
      </header>
      <div className="p-3 font-mono text-[12px] text-(--muted)">{reason}</div>
    </section>
  );
}
