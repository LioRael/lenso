export type AvailableModuleRegistryCatalog = {
  version: number;
  modules: AvailableModuleRegistryEntry[];
};

export type AvailableModuleRegistryEntry = {
  name: string;
  version: string;
  source: "remote" | string;
  manifestReference: string;
  baseUrl?: string;
  capabilities?: string[];
  consolePackages?: AvailableModuleConsolePackageHint[];
  summary?: string;
};

export type AvailableModuleConsolePackageHint = {
  packageName: string;
  exportName: string;
  route?: string;
};

export type AvailableModuleManifestSnapshot = {
  name: string;
  version: string;
  source: "remote" | string;
  consolePackages?: AvailableModuleConsolePackageHint[];
};

export type AvailableModulePreflightStatus =
  | "unknown"
  | "ready"
  | "needs_base_url"
  | "manifest_mismatch"
  | "package_hint_mismatch";

export type AvailableModuleRegistryRow = {
  key: string;
  name: string;
  version: string;
  source: string;
  manifestReference: string;
  baseUrl: string;
  capabilityCount: number;
  consolePackageHintCount: number;
  preflightStatus: AvailableModulePreflightStatus;
  preflightLabel: string;
  preflightReason: string;
  summary: string;
};

export type AvailableModuleManifestSnapshots = Record<
  string,
  AvailableModuleManifestSnapshot | undefined
>;

const statusLabel: Record<AvailableModulePreflightStatus, string> = {
  manifest_mismatch: "manifest mismatch",
  needs_base_url: "needs base URL",
  package_hint_mismatch: "package hint mismatch",
  ready: "ready",
  unknown: "unknown",
};

export function availableModuleRegistryRows(
  catalog: AvailableModuleRegistryCatalog,
  manifests: AvailableModuleManifestSnapshots = {}
): AvailableModuleRegistryRow[] {
  return catalog.modules.map((entry) => {
    const manifest = manifests[entry.name];
    const preflight = availableModulePreflight(entry, manifest);
    return {
      baseUrl: entry.baseUrl ?? "-",
      capabilityCount: entry.capabilities?.length ?? 0,
      consolePackageHintCount: entry.consolePackages?.length ?? 0,
      key: `${entry.name}:${entry.version}:${entry.manifestReference}`,
      manifestReference: entry.manifestReference,
      name: entry.name,
      preflightLabel: statusLabel[preflight.status],
      preflightReason: preflight.reason,
      preflightStatus: preflight.status,
      source: entry.source,
      summary: entry.summary ?? "-",
      version: entry.version,
    };
  });
}

function availableModulePreflight(
  entry: AvailableModuleRegistryEntry,
  manifest: AvailableModuleManifestSnapshot | undefined
): { reason: string; status: AvailableModulePreflightStatus } {
  if (
    !entry.baseUrl &&
    !/^https?:\/\/.+\/manifest$/u.test(entry.manifestReference)
  ) {
    return {
      reason:
        "registry install needs baseUrl when the manifest reference is not an HTTP /manifest URL",
      status: "needs_base_url",
    };
  }

  if (!manifest) {
    return {
      reason: "run lenso module registry doctor to fetch and validate manifest",
      status: "unknown",
    };
  }

  if (
    manifest.name !== entry.name ||
    manifest.version !== entry.version ||
    manifest.source !== entry.source
  ) {
    return {
      reason: "catalog identity does not match the fetched module manifest",
      status: "manifest_mismatch",
    };
  }

  const manifestPackages = new Set(
    (manifest.consolePackages ?? []).map(consolePackageKey)
  );
  const hasMismatchedPackageHint = (entry.consolePackages ?? []).some(
    (hint) => !manifestPackages.has(consolePackageKey(hint))
  );
  if (hasMismatchedPackageHint) {
    return {
      reason:
        "catalog console package hints drift from manifest console declarations",
      status: "package_hint_mismatch",
    };
  }

  return {
    reason: "catalog entry matches the fetched remote manifest snapshot",
    status: "ready",
  };
}

function consolePackageKey(hint: AvailableModuleConsolePackageHint): string {
  return `${hint.packageName}#${hint.exportName}`;
}
