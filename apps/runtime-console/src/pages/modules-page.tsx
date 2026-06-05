import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Boxes,
  KeyRound,
  RefreshCw,
  Route,
  ScrollText,
  ShieldCheck,
  TriangleAlert,
  Zap,
} from "lucide-react";
import { useState } from "react";

import { Button } from "../components/ui/button";
import { cn } from "../lib/cn";
import {
  httpClient,
  isApiMode,
  runtimeConsoleDataSource,
} from "../lib/http-client";
import {
  type AdminModuleMetadata,
  type ModuleRegistryFilters,
  adminSurfaceLabel,
  adminSurfaceMetadataRows,
  filterModuleRegistry,
  moduleActivationLabel,
  moduleErrorMessage,
  moduleGovernanceRows,
  moduleHttpRouteRows,
  moduleIsLoaded,
  moduleManifestCheckGroups,
  moduleRegistrySummary,
  moduleRuntimeFunctionRows,
  moduleManifestChecks,
  moduleManifestHealth,
  moduleStatusLabel,
  storyDisplayRows,
} from "./data-render-model";

type ModulesResponse = {
  modules: AdminModuleMetadata[];
  refreshed_at: string | null;
  refresh_error: string | null;
};

const modulesQueryKey = ["modules", "registry"] as const;
const emptyModules: AdminModuleMetadata[] = [];

export function ModulesPage() {
  if (!isApiMode()) {
    return <ModulesPlaceholder reason="modules registry requires API mode" />;
  }
  return <ModulesContent />;
}

function ModulesContent() {
  const queryClient = useQueryClient();
  const modulesQuery = useQuery({
    enabled: isApiMode(),
    queryKey: modulesQueryKey,
    queryFn: () => httpClient.get("admin/data/modules").json<ModulesResponse>(),
  });
  const refreshMutation = useMutation({
    mutationFn: () =>
      httpClient.post("admin/data/modules/refresh").json<ModulesResponse>(),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: modulesQueryKey });
    },
  });
  const modules = modulesQuery.data?.modules ?? emptyModules;
  const [selectedModuleName, setSelectedModuleName] = useState<string | null>(
    null
  );
  const [filters, setFilters] = useState<ModuleRegistryFilters>({
    query: "",
    lint: "all",
    source: "all",
    status: "all",
  });
  const summary = moduleRegistrySummary(modules);
  const filteredModules = filterModuleRegistry(modules, filters);
  const selectedModule =
    filteredModules.find(
      (module) => module.module_name === selectedModuleName
    ) ??
    filteredModules[0] ??
    null;

  return (
    <section className="grid h-full min-h-0 grid-rows-[auto_minmax(0,1fr)] overflow-hidden bg-(--background) text-(--foreground)">
      <header className="border-b border-(--border-subtle) bg-(--surface) px-3 py-2">
        <div className="flex items-center gap-2">
          <Boxes className="text-(--accent)" size={14} />
          <h1 className="font-mono text-[13px] font-semibold">Modules</h1>
          <span className="ml-auto font-mono text-[10px] text-(--muted)">
            {modules.length} modules / {runtimeConsoleDataSource()} /{" "}
            {registrySnapshotLabel(modulesQuery.data?.refreshed_at ?? null)}
          </span>
          <Button
            aria-label="Refresh module registry"
            className="min-h-6 px-2"
            disabled={refreshMutation.isPending}
            onClick={() => refreshMutation.mutate()}
            title="Refresh module registry"
            type="button"
            variant="ghost"
          >
            <RefreshCw
              className={cn(refreshMutation.isPending && "animate-spin")}
              size={13}
            />
            Refresh
          </Button>
        </div>
        {modulesQuery.data?.refresh_error ? (
          <p className="mt-1 font-mono text-[10px] text-(--error)">
            Registry refresh error: {modulesQuery.data.refresh_error}
          </p>
        ) : refreshMutation.isError ? (
          <p className="mt-1 font-mono text-[10px] text-(--error)">
            Refresh failed: {String(refreshMutation.error.message)}
          </p>
        ) : null}
      </header>

      <div className="grid min-h-0 grid-cols-[260px_minmax(0,1fr)] overflow-hidden">
        <nav className="min-h-0 overflow-auto border-r border-(--border-subtle) p-2 font-mono text-[12px]">
          <ModuleRegistryControls
            filters={filters}
            onChange={setFilters}
            summary={summary}
          />
          {modulesQuery.isLoading ? (
            <p className="px-2 py-1 text-(--muted)">Loading...</p>
          ) : modulesQuery.isError ? (
            <p className="px-2 py-1 text-(--error)">Failed to load modules.</p>
          ) : modules.length === 0 ? (
            <p className="px-2 py-1 text-(--muted)">No modules registered.</p>
          ) : filteredModules.length === 0 ? (
            <p className="px-2 py-2 text-(--muted)">No modules match.</p>
          ) : (
            filteredModules.map((module) => {
              const selected =
                selectedModule?.module_name === module.module_name;
              const lintHealth = moduleManifestHealth(module);
              return (
                <button
                  className={cn(
                    "block w-full border-l-2 px-2 py-1 text-left",
                    selected
                      ? "bg-(--accent-soft) shadow-[inset_2px_0_0_var(--accent)]"
                      : "hover:bg-(--sidebar)",
                    lintHealth === "ok" && "border-l-(--success)",
                    lintHealth === "warning" && "border-l-(--warning)",
                    lintHealth === "error" && "border-l-(--error)",
                    moduleIsLoaded(module) ? null : "text-(--secondary)"
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
                    <span
                      className={cn(
                        "ml-auto shrink-0 border px-1 text-[9px] uppercase",
                        lintHealth === "ok" &&
                          "border-[color-mix(in_srgb,var(--success)_45%,transparent)] text-(--success)",
                        lintHealth === "warning" &&
                          "border-[color-mix(in_srgb,var(--warning)_55%,transparent)] text-(--warning)",
                        lintHealth === "error" &&
                          "border-[color-mix(in_srgb,var(--error)_55%,transparent)] text-(--error)"
                      )}
                    >
                      {lintHealth}
                    </span>
                  </span>
                  <span className="block truncate text-[10px] text-(--muted)">
                    {module.source} / {adminSurfaceLabel(module.admin)} /{" "}
                    {moduleStatusLabel(module)} /{" "}
                    {moduleActivationLabel(module)}
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

function ModuleRegistryControls({
  filters,
  onChange,
  summary,
}: {
  filters: ModuleRegistryFilters;
  onChange: (filters: ModuleRegistryFilters) => void;
  summary: ReturnType<typeof moduleRegistrySummary>;
}) {
  return (
    <div className="mb-2 grid gap-2 border-b border-(--border-subtle) pb-2">
      <div className="grid grid-cols-5 gap-1 text-center text-[10px]">
        <Counter label="total" value={summary.total} />
        <Counter label="linked" value={summary.linked} />
        <Counter label="remote" value={summary.remote} />
        <Counter label="lint warn" value={summary.lint_warning} />
        <Counter label="lint err" value={summary.lint_error} tone="error" />
      </div>
      <input
        aria-label="Search module registry"
        className="h-7 w-full border border-(--border-subtle) bg-(--background) px-2 text-[11px] text-(--foreground) outline-none placeholder:text-(--muted) focus:border-(--accent)"
        onChange={(event) =>
          onChange({ ...filters, query: event.currentTarget.value })
        }
        placeholder="search modules, routes, capabilities"
        type="search"
        value={filters.query}
      />
      <div className="grid gap-1">
        <SegmentedFilter
          label="source"
          onChange={(source) =>
            onChange({
              ...filters,
              source: source as ModuleRegistryFilters["source"],
            })
          }
          options={["all", "linked", "remote"]}
          value={filters.source}
        />
        <SegmentedFilter
          label="status"
          onChange={(status) =>
            onChange({
              ...filters,
              status: status as ModuleRegistryFilters["status"],
            })
          }
          options={["all", "loaded", "error"]}
          value={filters.status}
        />
        <SegmentedFilter
          label="lint"
          onChange={(lint) =>
            onChange({
              ...filters,
              lint: lint as ModuleRegistryFilters["lint"],
            })
          }
          options={["all", "ok", "warn", "err"]}
          value={lintFilterLabel(filters.lint)}
        />
      </div>
    </div>
  );
}

function SegmentedFilter({
  label,
  onChange,
  options,
  value,
}: {
  label: string;
  onChange: (value: string) => void;
  options: string[];
  value: string;
}) {
  return (
    <div className="grid grid-cols-[44px_minmax(0,1fr)] items-center gap-1">
      <span className="truncate text-[9px] uppercase text-(--muted)">
        {label}
      </span>
      <div className="grid auto-cols-fr grid-flow-col gap-1">
        {options.map((option) => (
          <button
            aria-pressed={value === option}
            className={cn(
              "h-6 min-w-0 truncate border border-(--border-subtle) px-1 text-[10px] text-(--muted)",
              value === option
                ? "bg-(--accent-soft) text-(--foreground)"
                : "bg-(--background) hover:bg-(--sidebar)"
            )}
            key={option}
            onClick={() => onChange(expandLintFilter(option))}
            title={`${label}: ${option}`}
            type="button"
          >
            {option}
          </button>
        ))}
      </div>
    </div>
  );
}

function lintFilterLabel(value: ModuleRegistryFilters["lint"]): string {
  if (value === "warning") {
    return "warn";
  }
  if (value === "error") {
    return "err";
  }
  return value;
}

function expandLintFilter(value: string): string {
  if (value === "warn") {
    return "warning";
  }
  if (value === "err") {
    return "error";
  }
  return value;
}

function Counter({
  label,
  tone = "default",
  value,
}: {
  label: string;
  tone?: "default" | "error";
  value: number;
}) {
  return (
    <div className="border border-(--border-subtle) bg-(--surface) px-1 py-1">
      <div
        className={cn(
          "truncate text-[11px] text-(--secondary)",
          tone === "error" && value > 0 && "text-(--error)"
        )}
      >
        {value}
      </div>
      <div className="truncate text-[9px] text-(--muted)">{label}</div>
    </div>
  );
}

function registrySnapshotLabel(refreshedAt: string | null): string {
  if (!refreshedAt) {
    return "not refreshed";
  }
  const timestamp = new Date(refreshedAt);
  if (Number.isNaN(timestamp.getTime())) {
    return refreshedAt;
  }
  return timestamp.toLocaleString();
}

function ModuleRegistryDetail({ module }: { module: AdminModuleMetadata }) {
  const routeRows = moduleHttpRouteRows(module);
  const runtimeRows = moduleRuntimeFunctionRows(module);
  const manifestChecks = moduleManifestChecks(module);
  const storyRows = storyDisplayRows(module);
  return (
    <div className="grid gap-3">
      <section className="border border-(--border-subtle) bg-(--surface)">
        <header className="flex items-center gap-2 border-b border-(--border-subtle) px-3 py-2 font-semibold">
          <Boxes className="text-(--info)" size={14} />
          <span>{module.module_name}</span>
          <span className="ml-auto border border-(--border-subtle) px-2 py-0.5 text-[10px] text-(--secondary)">
            {module.source} / {moduleStatusLabel(module)} /{" "}
            {moduleActivationLabel(module)}
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

      <ModuleGovernancePanel module={module} />
      <ModuleCapabilitiesList capabilities={module.capabilities} />
      <ModuleStoryDisplayTable rows={storyRows} />
      <ModuleRuntimeFunctionsTable rows={runtimeRows} />
      <ModuleManifestChecks checks={manifestChecks} />
      <ModuleHttpRoutesTable rows={routeRows} />
    </div>
  );
}

function ModuleGovernancePanel({ module }: { module: AdminModuleMetadata }) {
  const issues = module.governance?.capability_issues ?? [];
  return (
    <section className="min-w-0 border border-(--border-subtle) bg-(--surface)">
      <header className="flex items-center gap-2 border-b border-(--border-subtle) px-3 py-2 font-semibold">
        <ShieldCheck className="text-(--accent)" size={14} />
        <span>Governance</span>
        <span
          className={cn(
            "ml-auto border px-1.5 py-0.5 text-[10px]",
            moduleActivationLabel(module) === "active" &&
              "border-[color-mix(in_srgb,var(--success)_45%,transparent)] text-(--success)",
            moduleActivationLabel(module) === "needs attention" &&
              "border-[color-mix(in_srgb,var(--warning)_55%,transparent)] text-(--warning)",
            moduleActivationLabel(module) === "blocked" &&
              "border-[color-mix(in_srgb,var(--error)_55%,transparent)] text-(--error)"
          )}
        >
          {moduleActivationLabel(module)}
        </span>
      </header>
      <MetadataRows rows={moduleGovernanceRows(module)} />
      {issues.length > 0 ? (
        <div className="grid gap-1 border-t border-(--border-subtle) px-3 py-2">
          {issues.slice(0, 4).map((issue) => (
            <div
              className="grid min-w-0 grid-cols-[minmax(0,180px)_minmax(0,1fr)] gap-2 text-[11px]"
              key={`${issue.subject}:${issue.capability}`}
              title={issue.suggestion}
            >
              <span className="truncate text-(--warning)">
                {issue.capability}
              </span>
              <span className="truncate text-(--muted)">{issue.subject}</span>
            </div>
          ))}
        </div>
      ) : null}
    </section>
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

function ModuleCapabilitiesList({ capabilities }: { capabilities: string[] }) {
  if (capabilities.length === 0) {
    return (
      <section className="border border-(--border-subtle) bg-(--surface) px-3 py-2 text-(--muted)">
        No capabilities declared.
      </section>
    );
  }

  return (
    <section className="min-w-0 border border-(--border-subtle) bg-(--surface)">
      <header className="flex items-center gap-2 border-b border-(--border-subtle) px-3 py-2 font-semibold">
        <KeyRound className="text-(--warning)" size={14} />
        <span>Capabilities</span>
        <span className="ml-auto border border-(--border-subtle) px-1.5 py-0.5 text-[10px] text-(--secondary)">
          {capabilities.length}
        </span>
      </header>
      <div className="flex flex-wrap gap-1.5 p-2">
        {capabilities.map((capability) => (
          <span
            className="max-w-full truncate border border-(--border-subtle) bg-(--sidebar) px-2 py-1 text-[11px] text-(--secondary)"
            key={capability}
            title={capability}
          >
            {capability}
          </span>
        ))}
      </div>
    </section>
  );
}

function ModuleStoryDisplayTable({
  rows,
}: {
  rows: ReturnType<typeof storyDisplayRows>;
}) {
  if (rows.length === 0) {
    return (
      <section className="border border-(--border-subtle) bg-(--surface) px-3 py-2 text-(--muted)">
        No story display descriptors declared.
      </section>
    );
  }

  return (
    <section className="min-w-0 border border-(--border-subtle) bg-(--surface)">
      <header className="flex items-center gap-2 border-b border-(--border-subtle) px-3 py-2 font-semibold">
        <ScrollText className="text-(--info)" size={14} />
        <span>Story Display</span>
        <span className="ml-auto border border-(--border-subtle) px-1.5 py-0.5 text-[10px] text-(--secondary)">
          {rows.length}
        </span>
      </header>
      <div className="overflow-auto">
        <table className="w-full min-w-[680px] table-fixed">
          <thead className="bg-(--sidebar) text-[10px] uppercase tracking-wide text-(--muted)">
            <tr>
              <th className="px-3 py-1.5 text-left">source</th>
              <th className="px-3 py-1.5 text-left">display</th>
              <th className="px-3 py-1.5 text-left">story</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((row) => (
              <tr
                className="border-t border-(--border-subtle) text-[11px]"
                key={row.key}
              >
                <td className="truncate px-3 py-1.5 text-(--foreground)">
                  {row.source}
                </td>
                <td className="truncate px-3 py-1.5 text-(--secondary)">
                  {row.displayName}
                </td>
                <td className="truncate px-3 py-1.5 text-(--muted)">
                  {row.storyTitle}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  );
}

function ModuleRuntimeFunctionsTable({
  rows,
}: {
  rows: ReturnType<typeof moduleRuntimeFunctionRows>;
}) {
  if (rows.length === 0) {
    return (
      <section className="border border-(--border-subtle) bg-(--surface) px-3 py-2 text-(--muted)">
        No runtime functions declared.
      </section>
    );
  }

  return (
    <section className="min-w-0 border border-(--border-subtle) bg-(--surface)">
      <header className="flex items-center gap-2 border-b border-(--border-subtle) px-3 py-2 font-semibold">
        <Zap className="text-(--success)" size={14} />
        <span>Runtime Functions</span>
        <span className="ml-auto border border-(--border-subtle) px-1.5 py-0.5 text-[10px] text-(--secondary)">
          {rows.length}
        </span>
      </header>
      <div className="overflow-auto">
        <table className="w-full min-w-[780px] table-fixed">
          <thead className="bg-(--sidebar) text-[10px] uppercase tracking-wide text-(--muted)">
            <tr>
              <th className="px-3 py-1.5 text-left">function</th>
              <th className="w-20 px-3 py-1.5 text-left">version</th>
              <th className="px-3 py-1.5 text-left">queue</th>
              <th className="px-3 py-1.5 text-left">input schema</th>
              <th className="px-3 py-1.5 text-left">retry</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((row) => (
              <tr
                className="border-t border-(--border-subtle) text-[11px]"
                key={row.key}
              >
                <td className="truncate px-3 py-1.5 text-(--foreground)">
                  {row.name}
                </td>
                <td className="px-3 py-1.5 text-(--secondary)">
                  {row.version}
                </td>
                <td className="truncate px-3 py-1.5 text-(--secondary)">
                  {row.queue}
                </td>
                <td className="truncate px-3 py-1.5 text-(--muted)">
                  {row.inputSchema}
                </td>
                <td className="truncate px-3 py-1.5 text-(--muted)">
                  {row.retryPolicy}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  );
}

function ModuleManifestChecks({
  checks,
}: {
  checks: ReturnType<typeof moduleManifestChecks>;
}) {
  const groups = moduleManifestCheckGroups(checks);
  return (
    <section className="min-w-0 border border-(--border-subtle) bg-(--surface)">
      <header className="flex items-center gap-2 border-b border-(--border-subtle) px-3 py-2 font-semibold">
        <TriangleAlert className="text-(--warning)" size={14} />
        <span>Manifest Lints</span>
        <span className="ml-auto border border-(--border-subtle) px-1.5 py-0.5 text-[10px] text-(--secondary)">
          {checks.length}
        </span>
      </header>
      {groups.length === 0 ? (
        <p className="px-3 py-2 text-(--muted)">No manifest lints returned.</p>
      ) : (
        <div className="divide-y divide-(--border-subtle)">
          {groups.map((group) => (
            <div className="grid gap-1 px-3 py-2" key={group.severity}>
              <div className="flex items-center gap-2 text-[10px] uppercase text-(--muted)">
                <span
                  className={cn(
                    "h-1.5 w-1.5 shrink-0",
                    group.severity === "ok" && "bg-(--success)",
                    group.severity === "warning" && "bg-(--warning)",
                    group.severity === "error" && "bg-(--error)"
                  )}
                />
                <span>{group.severity}</span>
                <span className="ml-auto text-(--secondary)">
                  {group.checks.length}
                </span>
              </div>
              <div className="grid gap-1">
                {group.checks.map((check) => (
                  <ManifestLintRow check={check} key={check.key} />
                ))}
              </div>
            </div>
          ))}
        </div>
      )}
    </section>
  );
}

function ManifestLintRow({
  check,
}: {
  check: ReturnType<typeof moduleManifestChecks>[number];
}) {
  return (
    <div className="grid min-w-0 grid-cols-[112px_minmax(0,170px)_minmax(0,1fr)] gap-x-2 gap-y-0.5 text-[11px]">
      <span
        className="truncate border border-(--border-subtle) bg-(--sidebar) px-1 text-[10px] text-(--secondary)"
        title={check.category}
      >
        {check.category}
      </span>
      <span className="truncate text-(--foreground)" title={check.subject}>
        {check.subject}
      </span>
      <span className="min-w-0 truncate text-(--muted)" title={check.message}>
        {check.message}
      </span>
      <span
        className="col-start-3 min-w-0 truncate text-[10px] text-(--secondary)"
        title={check.suggestion}
      >
        {check.suggestion}
      </span>
    </div>
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
