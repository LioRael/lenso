import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Boxes,
  Check,
  Copy,
  KeyRound,
  Network,
  RefreshCw,
  Route,
  ScrollText,
  ShieldCheck,
  SquareTerminal,
  TriangleAlert,
  Zap,
} from "lucide-react";
import { useState } from "react";

import { useConsoleCapabilities } from "../app/console-capabilities";
import {
  consolePackageInstallPlanFromMetadata,
  missingConsolePackagesFromMetadata,
} from "../app/console-module-metadata";
import { Button } from "../components/ui/button";
import {
  availableModulesPanelState,
  availableModulesQueryKey,
  availableModulesRows,
  fetchAvailableModules,
  moduleRefreshInvalidationQueryKeys,
} from "../data/available-modules";
import { useRemoteProxyCalls } from "../hooks/use-runtime-queries";
import { cn } from "../lib/cn";
import {
  httpClient,
  isApiMode,
  runtimeConsoleDataSource,
} from "../lib/http-client";
import {
  availableModuleHandoffState,
  availableModuleRowsFromResponse,
} from "./available-modules-model";
import {
  type AdminModuleMetadata,
  type ConfigValueMetadata,
  type ModuleRegistryFilters,
  adminSurfaceLabel,
  adminSurfaceMetadataRows,
  filterModuleRegistry,
  moduleActivationLabel,
  moduleActivationReasons,
  moduleConsoleSurfaceRows,
  moduleDisabledByConfig,
  moduleDesiredEnabled,
  moduleEntrypointRows,
  moduleEnabledConfigKey,
  moduleErrorMessage,
  moduleGovernanceRows,
  moduleHttpRouteRows,
  moduleIsLoaded,
  latestModuleRefreshResult,
  moduleRegistryHandoffCommands,
  moduleRegistryHandoffCopyLabel,
  moduleManifestCheckGroups,
  moduleRegistrySummary,
  moduleRestartPending,
  moduleRunningEnabled,
  moduleRuntimeFunctionRows,
  moduleManifestChecks,
  moduleManifestHealth,
  remoteModuleReadiness,
  moduleStatusLabel,
  storyDisplayRows,
} from "./data-render-model";
import { pushOperationsUrl } from "./operations-url-model";
import {
  flattenRemoteProxyCallPages,
  remoteProxyCallsPath,
  summarizeRemoteProxyCalls,
} from "./remote-proxy-calls-model";

type ModulesResponse = {
  modules: AdminModuleMetadata[];
  refreshed_at: string | null;
  refresh_error: string | null;
  refresh_history: ModuleRefreshRecord[];
};

type ModuleRefreshRecord = {
  id: string;
  status: "success" | "error" | string;
  started_at: string;
  completed_at: string;
  duration_ms: number;
  module_count: number;
  error: string | null;
  module_results: ModuleRefreshModuleResult[];
};

type ModuleRefreshModuleResult = {
  module_name: string;
  source: "linked" | "remote" | string;
  status: "loaded" | "error" | string;
  duration_ms?: number | null;
  endpoint?: string | null;
  error?: string | null;
};

type ConfigWriteResponse = {
  applies_on_restart: boolean;
};

type ConfigValueListResponse = {
  data: ConfigValueMetadata[];
};

const modulesQueryKey = ["modules", "registry"] as const;
const configValuesQueryKey = ["config", "values"] as const;
const emptyModules: AdminModuleMetadata[] = [];
const emptyConfigValues: ConfigValueMetadata[] = [];

function configPath(service: string, key: string) {
  return `admin/config/${encodeURIComponent(service)}/${encodeURIComponent(key)}`;
}

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
  const configValuesQuery = useQuery({
    enabled: isApiMode(),
    queryKey: configValuesQueryKey,
    queryFn: () =>
      httpClient.get("admin/config/values").json<ConfigValueListResponse>(),
  });
  const availableModulesQuery = useQuery({
    enabled: isApiMode(),
    queryKey: availableModulesQueryKey,
    queryFn: () => fetchAvailableModules(),
  });
  const refreshMutation = useMutation({
    mutationFn: () =>
      httpClient.post("admin/data/modules/refresh").json<ModulesResponse>(),
    onSuccess: async () => {
      await Promise.all(
        moduleRefreshInvalidationQueryKeys().map((queryKey) =>
          queryClient.invalidateQueries({ queryKey })
        )
      );
    },
  });
  const modules = modulesQuery.data?.modules ?? emptyModules;
  const configValues = configValuesQuery.data?.data ?? emptyConfigValues;
  const availableModuleRows = availableModulesQuery.data
    ? availableModuleRowsFromResponse(availableModulesQuery.data)
    : availableModulesRows();
  const availableModulePanelState = availableModulesPanelState({
    isError: availableModulesQuery.isError,
    isLoading: availableModulesQuery.isLoading,
    response: availableModulesQuery.data ?? null,
    rows: availableModuleRows,
  });
  const [selectedModuleName, setSelectedModuleName] = useState<string | null>(
    initialSelectedModuleName()
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
        <ModuleRefreshHistory
          history={modulesQuery.data?.refresh_history ?? []}
        />
      </header>

      <div className="grid min-h-0 grid-cols-[260px_minmax(0,1fr)] overflow-hidden">
        <nav className="min-h-0 overflow-auto border-r border-(--border-subtle) p-2 font-mono text-[12px]">
          <ModuleRegistryCatalogPanel
            configValues={configValues}
            modules={modules}
            panelState={availableModulePanelState}
            rows={availableModuleRows}
          />
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
            <ModuleRegistryDetail
              configValues={configValues}
              history={modulesQuery.data?.refresh_history ?? []}
              module={selectedModule}
            />
          ) : (
            <p className="text-(--muted)">Select a module.</p>
          )}
        </main>
      </div>
    </section>
  );
}

function ModuleRegistryCatalogPanel({
  configValues,
  modules,
  panelState,
  rows,
}: {
  configValues: ConfigValueMetadata[];
  modules: AdminModuleMetadata[];
  panelState: ReturnType<typeof availableModulesPanelState>;
  rows: ReturnType<typeof availableModulesRows>;
}) {
  const [copiedCommandKey, setCopiedCommandKey] = useState<string | null>(null);
  const packageHandoffCommands = moduleRegistryHandoffCommands().filter(
    (command) => command.key !== "add"
  );
  const copyCommand = (key: string, command: string) => {
    void window.navigator.clipboard?.writeText(command);
    setCopiedCommandKey(key);
    window.setTimeout(() => setCopiedCommandKey(null), 1200);
  };

  return (
    <section className="mb-2 min-w-0 border border-(--border-subtle) bg-(--surface)">
      <header className="flex items-center gap-1.5 border-b border-(--border-subtle) px-2 py-1.5">
        <SquareTerminal className="text-(--accent)" size={13} />
        <span className="truncate text-[10px] font-semibold uppercase text-(--secondary)">
          Available Modules
        </span>
        <span className="ml-auto border border-[color-mix(in_srgb,var(--info)_35%,transparent)] px-1.5 py-0.5 text-[9px] text-(--info)">
          Marketplace
        </span>
      </header>
      <div className="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-1 border-b border-(--border-subtle) px-2 py-1 text-[9px]">
        <span className="truncate text-(--muted)">{panelState.message}</span>
        <span
          className={cn(
            "shrink-0 border px-1 py-0.5",
            panelState.kind === "error"
              ? "border-[color-mix(in_srgb,var(--error)_55%,transparent)] text-(--error)"
              : "border-[color-mix(in_srgb,var(--info)_35%,transparent)] text-(--info)"
          )}
        >
          {panelState.label}
        </span>
      </div>
      <div className="grid gap-0.5 border-b border-(--border-subtle) px-2 py-1 text-[9px] text-(--muted)">
        <div className="truncate" title={panelState.source}>
          source {panelState.source}
        </div>
        <div className="truncate" title={panelState.detail}>
          {panelState.detail}
        </div>
      </div>
      <div className="grid gap-1 p-2">
        {panelState.kind === "loading" ||
        panelState.kind === "error" ||
        panelState.kind === "empty" ? (
          <div className="grid gap-1 border border-(--border-subtle) bg-(--background) px-2 py-1.5 text-[10px] text-(--muted)">
            <span>{panelState.message}</span>
            <div className="grid grid-cols-[minmax(0,1fr)_24px] items-center gap-1">
              <code
                className="truncate border border-(--border-subtle) bg-(--surface) px-1.5 py-1 text-[9px] text-(--secondary)"
                title={panelState.actionCommand}
              >
                {panelState.actionCommand}
              </code>
              <button
                aria-label={`${moduleRegistryHandoffCopyLabel(copiedCommandKey, "catalog-add")} catalog command`}
                className="grid size-6 place-items-center border border-(--border-subtle) bg-(--surface) text-(--muted) hover:bg-(--sidebar) hover:text-(--foreground)"
                onClick={() =>
                  copyCommand("catalog-add", panelState.actionCommand)
                }
                title={moduleRegistryHandoffCopyLabel(
                  copiedCommandKey,
                  "catalog-add"
                )}
                type="button"
              >
                {copiedCommandKey === "catalog-add" ? (
                  <Check size={11} />
                ) : (
                  <Copy size={11} />
                )}
              </button>
            </div>
          </div>
        ) : (
          rows.map((row) => {
            const [installCommand] = moduleRegistryHandoffCommands({
              manifestReference: row.manifestReference,
            });
            const installedModule = modules.find(
              (module) => module.module_name === row.name
            );
            const handoff = availableModuleHandoffState({
              installed: installedModule
                ? {
                    moduleName: installedModule.module_name,
                    restartPending: moduleRestartPending(
                      installedModule,
                      configValues
                    ),
                  }
                : null,
              installCommand: installCommand?.command ?? "",
              row,
            });
            const commandKey = `install:${row.key}`;
            return (
              <article
                className="grid gap-1 border border-(--border-subtle) bg-(--background) px-2 py-1.5"
                key={row.key}
              >
                <div className="flex min-w-0 items-center gap-1">
                  <span className="truncate text-[11px] text-(--foreground)">
                    {row.name}
                  </span>
                  <span className="shrink-0 text-[9px] text-(--muted)">
                    {row.version}
                  </span>
                  <span className="ml-auto shrink-0 border border-(--border-subtle) px-1 py-0.5 text-[9px] text-(--secondary)">
                    {handoff.label}
                  </span>
                </div>
                <div className="line-clamp-2 text-[9px] text-(--muted)">
                  {row.summary}
                </div>
                <div className="truncate text-[9px] text-(--muted)">
                  caps {row.capabilityCount} / console{" "}
                  {row.consolePackageHintCount} / {row.source}
                </div>
                <div className="truncate text-[9px] text-(--muted)">
                  base {row.baseUrl}
                </div>
                {handoff.kind === "installed" ||
                handoff.kind === "restart_pending" ? (
                  <button
                    className="border border-(--border-subtle) bg-(--surface) px-2 py-1 text-left text-[10px] text-(--secondary) hover:bg-(--sidebar) hover:text-(--foreground)"
                    onClick={() => window.location.assign(handoff.path)}
                    title={handoff.detail}
                    type="button"
                  >
                    <span className="block truncate">
                      {handoff.kind === "installed"
                        ? "Open Module"
                        : "Open Restart Step"}
                    </span>
                    <span className="block truncate pt-0.5 text-[9px] text-(--muted)">
                      {handoff.detail}
                    </span>
                  </button>
                ) : installCommand ? (
                  <div className="grid grid-cols-[minmax(0,1fr)_24px] items-center gap-1">
                    <code
                      className="truncate border border-(--border-subtle) bg-(--surface) px-1.5 py-1 text-[9px] text-(--secondary)"
                      title={installCommand.command}
                    >
                      {installCommand.command}
                    </code>
                    <button
                      aria-label={`${moduleRegistryHandoffCopyLabel(copiedCommandKey, commandKey)} install command`}
                      className="grid size-6 place-items-center border border-(--border-subtle) bg-(--surface) text-(--muted) hover:bg-(--sidebar) hover:text-(--foreground)"
                      onClick={() =>
                        copyCommand(commandKey, installCommand.command)
                      }
                      title={moduleRegistryHandoffCopyLabel(
                        copiedCommandKey,
                        commandKey
                      )}
                      type="button"
                    >
                      {copiedCommandKey === commandKey ? (
                        <Check size={11} />
                      ) : (
                        <Copy size={11} />
                      )}
                    </button>
                  </div>
                ) : null}
                {handoff.kind === "available" ? (
                  <div className="truncate text-[9px] text-(--muted)">
                    then apply-plan / pnpm install / restart
                  </div>
                ) : null}
              </article>
            );
          })
        )}
      </div>
      <div className="grid gap-1 border-t border-(--border-subtle) p-2">
        {packageHandoffCommands.map((command) => (
          <div
            className="grid grid-cols-[52px_minmax(0,1fr)_24px] items-center gap-1"
            key={command.key}
          >
            <span className="truncate text-[9px] uppercase text-(--muted)">
              {command.label}
            </span>
            <code
              className="truncate border border-(--border-subtle) bg-(--background) px-1.5 py-1 text-[9px] text-(--secondary)"
              title={command.command}
            >
              {command.command}
            </code>
            <button
              aria-label={`${moduleRegistryHandoffCopyLabel(copiedCommandKey, command.key)} ${command.label} command`}
              className="grid size-6 place-items-center border border-(--border-subtle) bg-(--background) text-(--muted) hover:bg-(--sidebar) hover:text-(--foreground)"
              onClick={() => copyCommand(command.key, command.command)}
              title={moduleRegistryHandoffCopyLabel(
                copiedCommandKey,
                command.key
              )}
              type="button"
            >
              {copiedCommandKey === command.key ? (
                <Check size={11} />
              ) : (
                <Copy size={11} />
              )}
            </button>
          </div>
        ))}
      </div>
    </section>
  );
}

function initialSelectedModuleName(): string | null {
  if (typeof window === "undefined") {
    return null;
  }
  return new URLSearchParams(window.location.search).get("module");
}

function ModuleRefreshHistory({ history }: { history: ModuleRefreshRecord[] }) {
  if (history.length === 0) {
    return null;
  }

  return (
    <div className="mt-1 flex min-w-0 gap-1 overflow-hidden font-mono text-[10px]">
      {history.slice(0, 3).map((record) => (
        <span
          className={cn(
            "min-w-0 truncate border px-1.5 py-0.5",
            record.status === "success" &&
              "border-[color-mix(in_srgb,var(--success)_40%,transparent)] text-(--success)",
            record.status === "error" &&
              "border-[color-mix(in_srgb,var(--error)_45%,transparent)] text-(--error)"
          )}
          key={record.id}
          title={refreshRecordTitle(record)}
        >
          refresh {record.status} / {record.module_count} modules /{" "}
          {record.duration_ms}ms
        </span>
      ))}
    </div>
  );
}

function refreshRecordTitle(record: ModuleRefreshRecord) {
  if (record.error) {
    return record.error;
  }
  if (record.module_results.length === 0) {
    return record.completed_at;
  }
  return record.module_results
    .slice(0, 5)
    .map((result) =>
      [
        result.module_name,
        result.status,
        result.duration_ms === null || result.duration_ms === undefined
          ? null
          : `${result.duration_ms}ms`,
        result.error,
      ]
        .filter(Boolean)
        .join(" / ")
    )
    .join("\n");
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

function ModuleRegistryDetail({
  configValues,
  history,
  module,
}: {
  configValues: ConfigValueMetadata[];
  history: ModuleRefreshRecord[];
  module: AdminModuleMetadata;
}) {
  const availableCapabilities = useConsoleCapabilities();
  const routeRows = moduleHttpRouteRows(module);
  const runtimeRows = moduleRuntimeFunctionRows(module);
  const manifestChecks = moduleManifestChecks(module);
  const consoleRows = moduleConsoleSurfaceRows(module, {
    availableCapabilities,
  });
  const missingConsolePackages = missingConsolePackagesFromMetadata([module]);
  const consolePackageInstallPlan = consolePackageInstallPlanFromMetadata([
    module,
  ]);
  const restartPending = moduleRestartPending(module, configValues);
  const entrypointRows = moduleEntrypointRows(module, {
    hasMissingConsolePackages: missingConsolePackages.length > 0,
    restartPending,
  });
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

      <ModuleEntrypointsPanel rows={entrypointRows} />
      <ModuleOperationsPanel
        configValues={configValues}
        history={history}
        module={module}
      />
      <ModuleGovernancePanel module={module} />
      <ModuleCapabilitiesList capabilities={module.capabilities} />
      <ModuleConsoleSurfacesTable rows={consoleRows} />
      <MissingConsolePackagesTable
        installPlan={consolePackageInstallPlan}
        rows={missingConsolePackages}
      />
      <ModuleStoryDisplayTable rows={storyRows} />
      <ModuleRuntimeFunctionsTable rows={runtimeRows} />
      <ModuleManifestChecks checks={manifestChecks} />
      <ModuleHttpRoutesTable rows={routeRows} />
    </div>
  );
}

function ModuleEntrypointsPanel({
  rows,
}: {
  rows: ReturnType<typeof moduleEntrypointRows>;
}) {
  if (rows.length === 0) {
    return null;
  }

  return (
    <section className="min-w-0 border border-(--border-subtle) bg-(--surface)">
      <header className="flex items-center gap-2 border-b border-(--border-subtle) px-3 py-2 font-semibold">
        <SquareTerminal className="text-(--accent)" size={14} />
        <span>Open Module</span>
        <span className="ml-auto border border-(--border-subtle) px-1.5 py-0.5 text-[10px] text-(--secondary)">
          {rows.length}
        </span>
      </header>
      <div className="grid gap-1 p-2 sm:grid-cols-2 xl:grid-cols-4">
        {rows.map((row) => (
          <button
            className={cn(
              "min-w-0 border border-(--border-subtle) bg-(--background) px-2 py-2 text-left hover:bg-(--sidebar)",
              !row.path && "cursor-default"
            )}
            disabled={!row.path}
            key={row.key}
            onClick={() => openModuleEntrypoint(row.path)}
            title={row.detail}
            type="button"
          >
            <span className="block truncate text-[11px] text-(--foreground)">
              {row.label}
            </span>
            <span className="block truncate pt-1 text-[10px] text-(--muted)">
              {row.detail}
            </span>
          </button>
        ))}
      </div>
    </section>
  );
}

function openModuleEntrypoint(path: string) {
  if (!path) {
    return;
  }
  if (path.startsWith("#")) {
    document.querySelector(path)?.scrollIntoView({ block: "start" });
    return;
  }
  if (path.startsWith("/operations/")) {
    pushOperationsUrl(path);
    return;
  }
  window.location.assign(path);
}

function MissingConsolePackagesTable({
  installPlan,
  rows,
}: {
  installPlan: ReturnType<typeof consolePackageInstallPlanFromMetadata>;
  rows: ReturnType<typeof missingConsolePackagesFromMetadata>;
}) {
  if (rows.length === 0) {
    return null;
  }

  return (
    <section className="min-w-0 border border-(--border-subtle) bg-(--surface)">
      <header className="flex items-center gap-2 border-b border-(--border-subtle) px-3 py-2 font-semibold">
        <TriangleAlert className="text-(--warning)" size={14} />
        <span>Missing Console Packages</span>
        <span className="border border-[color-mix(in_srgb,var(--info)_35%,transparent)] px-1.5 py-0.5 text-[10px] text-(--info)">
          manual install
        </span>
        <span className="ml-auto border border-(--border-subtle) px-1.5 py-0.5 text-[10px] text-(--secondary)">
          {rows.length}
        </span>
      </header>
      <div className="border-b border-(--border-subtle) px-3 py-2 text-[11px] text-(--secondary)">
        Run <code>lenso console-package apply-plan</code>, then{" "}
        <code>pnpm --dir apps/runtime-console install</code>. Restart the API
        and worker after package changes.
      </div>
      <div className="overflow-auto">
        <table className="w-full min-w-[820px] table-fixed">
          <thead className="bg-(--sidebar) text-[10px] uppercase tracking-wide text-(--muted)">
            <tr>
              <th className="px-3 py-1.5 text-left">package</th>
              <th className="w-28 px-3 py-1.5 text-left">plan</th>
              <th className="px-3 py-1.5 text-left">surface</th>
              <th className="px-3 py-1.5 text-left">route</th>
              <th className="px-3 py-1.5 text-left">capabilities</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((row) => {
              const plan = installPlan.find((item) => item.key === row.key);
              return (
                <tr
                  className="border-t border-(--border-subtle) text-[11px]"
                  key={row.key}
                >
                  <td className="truncate px-3 py-1.5 text-(--foreground)">
                    {row.packageName} / {row.exportName}
                  </td>
                  <td className="px-3 py-1.5">
                    <span
                      className="inline-block max-w-full truncate border border-[color-mix(in_srgb,var(--info)_45%,transparent)] px-1.5 py-0.5 text-[10px] text-(--info)"
                      title={plan?.reason}
                    >
                      {plan?.status ?? "planned"}
                    </span>
                    <div
                      className="truncate pt-1 text-[9px] text-(--muted)"
                      title={
                        plan
                          ? `pnpm --dir apps/runtime-console add ${plan.packageName}`
                          : undefined
                      }
                    >
                      {plan
                        ? `pnpm --dir apps/runtime-console add ${plan.packageName}`
                        : "manual install required"}
                    </div>
                  </td>
                  <td className="truncate px-3 py-1.5 text-(--secondary)">
                    {row.moduleName} / {row.surfaceLabel} / {row.surfaceName}
                  </td>
                  <td className="truncate px-3 py-1.5 text-(--muted)">
                    {row.route}
                  </td>
                  <td className="truncate px-3 py-1.5 text-(--muted)">
                    {row.requiredCapabilities.join(", ") || "-"}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </section>
  );
}

function ModuleOperationsPanel({
  configValues,
  history,
  module,
}: {
  configValues: ConfigValueMetadata[];
  history: ModuleRefreshRecord[];
  module: AdminModuleMetadata;
}) {
  const queryClient = useQueryClient();
  const [moduleToggleMessage, setModuleToggleMessage] = useState<string | null>(
    null
  );
  const callsQuery = useRemoteProxyCalls({
    limit: 25,
    moduleName: module.module_name,
  });
  const calls = flattenRemoteProxyCallPages(callsQuery.data?.pages);
  const summary = summarizeRemoteProxyCalls(calls);
  const readiness = remoteModuleReadiness(module, calls);
  const { latestFailure } = readiness;
  const isRemote = module.source === "remote";
  const diagnostics =
    module.source_diagnostics?.kind === "remote"
      ? module.source_diagnostics
      : null;
  const latestRefresh = latestModuleRefreshResult(module, history);
  const desiredEnabled = moduleDesiredEnabled(module, configValues);
  const runningEnabled = moduleRunningEnabled(module);
  const restartPending = moduleRestartPending(module, configValues);
  const disabledByConfig = moduleDisabledByConfig(module);
  const moduleSupportsToggle =
    module.source === "linked" || module.source === "remote";
  const moduleToggleTarget =
    moduleSupportsToggle &&
    (moduleIsLoaded(module) ||
      disabledByConfig ||
      restartPending ||
      isRemote) &&
    desiredEnabled !== null
      ? !desiredEnabled
      : null;
  const moduleToggleMutation = useMutation({
    mutationFn: (enabled: boolean) =>
      httpClient
        .put(configPath("*", moduleEnabledConfigKey(module.module_name)), {
          json: { value: enabled },
        })
        .json<ConfigWriteResponse>(),
    onSuccess: async (response, enabled) => {
      setModuleToggleMessage(
        `${enabled ? "enable" : "disable"} saved${
          response.applies_on_restart ? "; restart required" : ""
        }`
      );
      await queryClient.invalidateQueries({ queryKey: modulesQueryKey });
      await queryClient.invalidateQueries({ queryKey: configValuesQueryKey });
    },
    onError: (error: unknown) => setModuleToggleMessage(errorMessage(error)),
  });
  const operationStatus = restartPending
    ? "pending restart"
    : isRemote
      ? readiness.status
      : moduleIsLoaded(module)
        ? "ready"
        : "blocked";

  return (
    <section className="min-w-0 border border-(--border-subtle) bg-(--surface)">
      <header className="flex items-center gap-2 border-b border-(--border-subtle) px-3 py-2 font-semibold">
        <Network className="text-(--accent)" size={14} />
        <span>Operations</span>
        <span
          className={cn(
            "ml-auto border px-1.5 py-0.5 text-[10px]",
            readiness.status === "ready" &&
              "border-[color-mix(in_srgb,var(--success)_45%,transparent)] text-(--success)",
            readiness.status === "degraded" &&
              "border-[color-mix(in_srgb,var(--warning)_55%,transparent)] text-(--warning)",
            readiness.status === "blocked" &&
              "border-[color-mix(in_srgb,var(--error)_55%,transparent)] text-(--error)",
            restartPending &&
              "border-[color-mix(in_srgb,var(--warning)_55%,transparent)] text-(--warning)"
          )}
        >
          {operationStatus}
        </span>
        {restartPending ? (
          <span className="border border-[color-mix(in_srgb,var(--warning)_55%,transparent)] px-1.5 py-0.5 text-[10px] text-(--warning)">
            pending restart
          </span>
        ) : null}
        {moduleToggleTarget === null ? null : (
          <Button
            className="min-h-6 px-2"
            disabled={moduleToggleMutation.isPending}
            onClick={() => moduleToggleMutation.mutate(moduleToggleTarget)}
            title={`${moduleToggleTarget ? "Enable" : "Disable"} ${module.module_name}`}
            type="button"
            variant="ghost"
          >
            <Zap size={12} />
            {moduleToggleTarget ? "Enable" : "Disable"}
          </Button>
        )}
        {isRemote ? (
          <button
            className="border border-(--border-subtle) bg-(--elevated) px-1.5 py-0.5 text-[10px] text-(--secondary) hover:text-(--foreground)"
            onClick={() =>
              pushOperationsUrl(
                remoteProxyCallsPath({ moduleName: module.module_name })
              )
            }
            type="button"
          >
            Remote Calls
          </button>
        ) : null}
      </header>
      {moduleToggleMessage ? (
        <p className="border-b border-(--border-subtle) px-3 py-2 text-[11px] text-(--warning)">
          {moduleToggleMessage}
        </p>
      ) : null}
      {isRemote && callsQuery.isError ? (
        <p className="border-b border-(--border-subtle) px-3 py-2 text-(--error)">
          Failed to load recent remote calls.
        </p>
      ) : isRemote && callsQuery.isLoading ? (
        <p className="border-b border-(--border-subtle) px-3 py-2 text-(--muted)">
          Loading recent remote calls...
        </p>
      ) : null}
      <MetadataRows
        rows={[
          { label: "readiness", value: operationStatus },
          {
            label: "running enabled",
            value: module.source === "linked" ? String(runningEnabled) : "-",
          },
          {
            label: "desired enabled",
            value: desiredEnabled === null ? "-" : String(desiredEnabled),
          },
          {
            label: "restart pending",
            value: restartPending ? "true" : "false",
          },
          {
            label: "next step",
            value: restartPending
              ? "restart API and worker to apply the desired module state"
              : isRemote
                ? "module is loaded; use declared host proxy endpoints"
                : "linked module state is current",
          },
          {
            label: "reason",
            value: isRemote
              ? readiness.reasons.join(" / ")
              : "linked module state is applied on service restart",
          },
          {
            label: "latest refresh",
            value: latestRefresh
              ? [
                  latestRefresh.status,
                  latestRefresh.durationMs === null
                    ? null
                    : `${latestRefresh.durationMs}ms`,
                  latestRefresh.completedAt,
                ]
                  .filter(Boolean)
                  .join(" / ")
              : "-",
          },
          {
            label: "refresh endpoint",
            value: latestRefresh?.endpoint ?? "-",
          },
          {
            label: "refresh error",
            value: latestRefresh?.error ?? "-",
          },
          { label: "base url", value: diagnostics?.base_url ?? "-" },
          { label: "manifest url", value: diagnostics?.manifest_url ?? "-" },
          {
            label: "timeout",
            value: diagnostics ? `${diagnostics.timeout_ms}ms` : "-",
          },
          {
            label: "load duration",
            value:
              diagnostics?.load_duration_ms === null ||
              diagnostics?.load_duration_ms === undefined
                ? "-"
                : `${diagnostics.load_duration_ms}ms`,
          },
          {
            label: "auth configured",
            value: diagnostics ? String(diagnostics.auth_configured) : "-",
          },
          {
            label: "last checked",
            value: diagnostics?.last_checked_at ?? "-",
          },
          {
            label: "last load error",
            value: diagnostics?.last_load_error ?? "-",
          },
          { label: "manifest", value: moduleManifestHealth(module) },
          { label: "activation", value: moduleActivationLabel(module) },
          { label: "http routes", value: String(module.http_routes.length) },
          {
            label: "runtime functions",
            value: String(module.runtime?.functions.length ?? 0),
          },
          { label: "recent calls", value: String(summary.total) },
          { label: "failed calls", value: String(summary.failed) },
          {
            label: "avg duration",
            value: formatRemoteDuration(summary.avgDurationMs),
          },
          {
            label: "p95 duration",
            value: formatRemoteDuration(summary.p95DurationMs),
          },
          {
            label: "latest failure",
            value: latestFailure
              ? [
                  latestFailure.error_code ?? "unknown_error",
                  latestFailure.remote_status
                    ? `status ${latestFailure.remote_status}`
                    : null,
                  latestFailure.occurred_at,
                ]
                  .filter(Boolean)
                  .join(" / ")
              : "-",
          },
        ]}
      />
    </section>
  );
}

function ModuleGovernancePanel({ module }: { module: AdminModuleMetadata }) {
  const issues = module.governance?.capability_issues ?? [];
  const activationReasons = moduleActivationReasons(module);
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
      {activationReasons.length > 0 ? (
        <div className="grid gap-1 border-t border-(--border-subtle) px-3 py-2">
          {activationReasons.slice(0, 4).map((reason) => (
            <div
              className="min-w-0 truncate text-[11px] text-(--warning)"
              key={reason}
              title={reason}
            >
              {reason}
            </div>
          ))}
        </div>
      ) : null}
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

function ModuleConsoleSurfacesTable({
  rows,
}: {
  rows: ReturnType<typeof moduleConsoleSurfaceRows>;
}) {
  if (rows.length === 0) {
    return (
      <section className="border border-(--border-subtle) bg-(--surface) px-3 py-2 text-(--muted)">
        No console surfaces declared.
      </section>
    );
  }

  return (
    <section className="min-w-0 border border-(--border-subtle) bg-(--surface)">
      <header className="flex items-center gap-2 border-b border-(--border-subtle) px-3 py-2 font-semibold">
        <Boxes className="text-(--accent)" size={14} />
        <span>Console Surfaces</span>
        <span className="ml-auto border border-(--border-subtle) px-1.5 py-0.5 text-[10px] text-(--secondary)">
          {rows.length}
        </span>
      </header>
      <div className="overflow-auto">
        <table className="w-full min-w-[980px] table-fixed">
          <thead className="bg-(--sidebar) text-[10px] uppercase tracking-wide text-(--muted)">
            <tr>
              <th className="w-28 px-3 py-1.5 text-left">area</th>
              <th className="w-40 px-3 py-1.5 text-left">status</th>
              <th className="px-3 py-1.5 text-left">surface</th>
              <th className="px-3 py-1.5 text-left">route</th>
              <th className="px-3 py-1.5 text-left">package</th>
              <th className="px-3 py-1.5 text-left">capabilities</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((row) => (
              <tr
                className="border-t border-(--border-subtle) text-[11px]"
                key={row.key}
              >
                <td className="truncate px-3 py-1.5 text-(--foreground)">
                  {row.area}
                </td>
                <td className="min-w-0 px-3 py-1.5">
                  <span
                    className={cn(
                      "inline-block max-w-full truncate border px-1.5 py-0.5 text-[10px]",
                      row.availability === "available" &&
                        "border-[color-mix(in_srgb,var(--success)_45%,transparent)] text-(--success)",
                      row.availability === "missing_capability" &&
                        "border-[color-mix(in_srgb,var(--warning)_55%,transparent)] text-(--warning)",
                      row.availability === "unsupported_package" &&
                        "border-[color-mix(in_srgb,var(--error)_55%,transparent)] text-(--error)"
                    )}
                    title={row.availabilityReason}
                  >
                    {row.availabilityLabel}
                  </span>
                  <div
                    className="truncate pt-1 text-[9px] text-(--muted)"
                    title={row.availabilityReason}
                  >
                    {row.availabilityReason}
                  </div>
                </td>
                <td className="truncate px-3 py-1.5 text-(--secondary)">
                  {row.label} / {row.name}
                </td>
                <td className="truncate px-3 py-1.5 text-(--muted)">
                  {row.route}
                </td>
                <td className="truncate px-3 py-1.5 text-(--muted)">
                  {row.packageName} / {row.exportName}
                  <div className="truncate pt-1 text-[9px] text-(--muted)">
                    {row.packageRegistration}
                  </div>
                </td>
                <td className="truncate px-3 py-1.5 text-(--muted)">
                  {row.capabilities}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
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
  const [copiedQueueKey, setCopiedQueueKey] = useState<string | null>(null);
  const copyQueueKey = (key: string, queueKey: string) => {
    void window.navigator.clipboard?.writeText(queueKey);
    setCopiedQueueKey(key);
    window.setTimeout(() => setCopiedQueueKey(null), 1200);
  };

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
      <div className="border-b border-(--border-subtle) px-3 py-2 text-[11px] text-(--muted)">
        Remote functions run through the host worker. Open the queue to inspect
        pending and failed runs.
      </div>
      <div className="overflow-auto">
        <table className="w-full min-w-[980px] table-fixed">
          <thead className="bg-(--sidebar) text-[10px] uppercase tracking-wide text-(--muted)">
            <tr>
              <th className="px-3 py-1.5 text-left">function</th>
              <th className="w-20 px-3 py-1.5 text-left">version</th>
              <th className="px-3 py-1.5 text-left">queue</th>
              <th className="px-3 py-1.5 text-left">worker queue</th>
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
                <td className="px-3 py-1.5">
                  <div className="grid grid-cols-[minmax(0,1fr)_24px_24px] items-center gap-1">
                    <button
                      className="min-w-0 truncate border border-(--border-subtle) bg-(--background) px-1.5 py-1 text-left font-mono text-[10px] text-(--secondary) hover:bg-(--sidebar) hover:text-(--foreground)"
                      onClick={() => pushOperationsUrl(row.queuePath)}
                      title={row.queueKey}
                      type="button"
                    >
                      {row.queueKey}
                    </button>
                    <button
                      aria-label={`${moduleRegistryHandoffCopyLabel(copiedQueueKey, row.key)} queue key`}
                      className="grid size-6 place-items-center border border-(--border-subtle) bg-(--background) text-(--muted) hover:bg-(--sidebar) hover:text-(--foreground)"
                      onClick={() => copyQueueKey(row.key, row.queueKey)}
                      title={moduleRegistryHandoffCopyLabel(
                        copiedQueueKey,
                        row.key
                      )}
                      type="button"
                    >
                      {copiedQueueKey === row.key ? (
                        <Check size={11} />
                      ) : (
                        <Copy size={11} />
                      )}
                    </button>
                    <button
                      aria-label="open queue"
                      className="grid size-6 place-items-center border border-(--border-subtle) bg-(--background) text-(--muted) hover:bg-(--sidebar) hover:text-(--foreground)"
                      onClick={() => pushOperationsUrl(row.queuePath)}
                      title="open queue"
                      type="button"
                    >
                      <SquareTerminal size={11} />
                    </button>
                  </div>
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
  const [copiedRouteKey, setCopiedRouteKey] = useState<string | null>(null);
  const copyRouteCommand = (key: string, command: string) => {
    void window.navigator.clipboard?.writeText(command);
    setCopiedRouteKey(key);
    window.setTimeout(() => setCopiedRouteKey(null), 1200);
  };

  if (rows.length === 0) {
    return (
      <section className="border border-(--border-subtle) bg-(--surface) px-3 py-2 text-(--muted)">
        No HTTP interfaces declared.
      </section>
    );
  }

  return (
    <section
      className="min-w-0 border border-(--border-subtle) bg-(--surface)"
      id="http-interfaces"
    >
      <header className="flex items-center gap-2 border-b border-(--border-subtle) px-3 py-2 font-semibold">
        <Route className="text-(--accent)" size={14} />
        <span>HTTP Interfaces</span>
        <span className="ml-auto border border-(--border-subtle) px-1.5 py-0.5 text-[10px] text-(--secondary)">
          {rows.length}
        </span>
      </header>
      <div className="overflow-auto">
        <table className="w-full min-w-[960px] table-fixed">
          <thead className="bg-(--sidebar) text-[10px] uppercase tracking-wide text-(--muted)">
            <tr>
              <th className="w-16 px-3 py-1.5 text-left">method</th>
              <th className="px-3 py-1.5 text-left">path</th>
              <th className="px-3 py-1.5 text-left">host proxy</th>
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
                <td className="px-3 py-1.5">
                  <div className="grid grid-cols-[minmax(0,1fr)_24px] items-center gap-1">
                    <code
                      className="truncate border border-(--border-subtle) bg-(--background) px-1.5 py-1 text-[10px] text-(--secondary)"
                      title={route.proxyCommand}
                    >
                      {route.proxyPath}
                    </code>
                    <button
                      aria-label={`${moduleRegistryHandoffCopyLabel(copiedRouteKey, route.key)} proxy command`}
                      className="grid size-6 place-items-center border border-(--border-subtle) bg-(--background) text-(--muted) hover:bg-(--sidebar) hover:text-(--foreground)"
                      onClick={() =>
                        copyRouteCommand(route.key, route.proxyCommand)
                      }
                      title={moduleRegistryHandoffCopyLabel(
                        copiedRouteKey,
                        route.key
                      )}
                      type="button"
                    >
                      {copiedRouteKey === route.key ? (
                        <Check size={11} />
                      ) : (
                        <Copy size={11} />
                      )}
                    </button>
                  </div>
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

function formatRemoteDuration(ms: number) {
  if (ms < 1000) {
    return `${ms}ms`;
  }
  return `${(ms / 1000).toFixed(1)}s`;
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : "Module operation failed";
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
