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
  installPolicy?: AvailableModuleRegistryInstallPolicy;
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

export type AvailableModuleRegistryDoctorSnapshot = {
  version: number;
  status: "passed" | "failed" | string;
  catalog: {
    modules: number;
    registryFile: string;
    version: number;
  };
  issues: AvailableModuleRegistryDoctorIssue[];
  modules: AvailableModuleRegistryDoctorModule[];
};

export type AvailableModuleRegistryDoctorIssue = {
  group: string;
  message: string;
  fix: string;
};

export type AvailableModuleRegistryDoctorModule = {
  name: string;
  source: "remote" | string;
  catalogVersion: string;
  manifestReference: string;
  baseUrl: string | null;
  consolePackageHints: number;
  installPolicy?: AvailableModuleRegistryInstallPolicy;
  manifestName: string | null;
  manifestStatus: "ok" | "invalid" | "unreadable" | string;
  manifestVersion: string | null;
  status: "ready" | "needs_attention" | string;
};

export type AvailableModulePreflightStatus =
  | "unknown"
  | "ready"
  | "review_required"
  | "needs_base_url"
  | "manifest_mismatch"
  | "package_hint_mismatch";

export type AvailableModuleRegistryInstallPolicy =
  | "review_required"
  | "trusted"
  | string;

export type AvailableModuleRegistryRow = {
  key: string;
  name: string;
  version: string;
  source: string;
  manifestReference: string;
  baseUrl: string;
  capabilityCount: number;
  consolePackageHintCount: number;
  installPolicy: AvailableModuleRegistryInstallPolicy;
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
  review_required: "review required",
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
      installPolicy: normalizeInstallPolicy(entry.installPolicy),
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

export function availableModuleRegistryRowsFromDoctorSnapshot(
  snapshot: AvailableModuleRegistryDoctorSnapshot
): AvailableModuleRegistryRow[] {
  return snapshot.modules.map((module) => {
    const preflight = availableModulePreflightFromDoctorSnapshot({
      issues: snapshot.issues,
      module,
    });
    return {
      baseUrl: module.baseUrl ?? "-",
      capabilityCount: 0,
      consolePackageHintCount: module.consolePackageHints,
      installPolicy: normalizeInstallPolicy(module.installPolicy),
      key: `${module.name}:${module.catalogVersion}:${module.manifestReference}`,
      manifestReference: module.manifestReference,
      name: module.name,
      preflightLabel: statusLabel[preflight.status],
      preflightReason: preflight.reason,
      preflightStatus: preflight.status,
      source: module.source,
      summary: "-",
      version: module.catalogVersion,
    };
  });
}

function availableModulePreflight(
  entry: AvailableModuleRegistryEntry,
  manifest: AvailableModuleManifestSnapshot | undefined
): { reason: string; status: AvailableModulePreflightStatus } {
  if (normalizeInstallPolicy(entry.installPolicy) !== "trusted") {
    return {
      reason:
        "registry install requires installPolicy trusted after operator review",
      status: "review_required",
    };
  }

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

function availableModulePreflightFromDoctorSnapshot({
  issues,
  module,
}: {
  issues: AvailableModuleRegistryDoctorIssue[];
  module: AvailableModuleRegistryDoctorModule;
}): { reason: string; status: AvailableModulePreflightStatus } {
  if (normalizeInstallPolicy(module.installPolicy) !== "trusted") {
    const issue = issues.find(
      (candidate) =>
        candidate.group === "Catalog" &&
        candidate.message.startsWith(`${module.name} installPolicy `)
    );
    return {
      reason:
        issue?.message ??
        "registry install requires installPolicy trusted after operator review",
      status: "review_required",
    };
  }

  if (module.status === "ready") {
    return {
      reason: "registry doctor snapshot passed for this module manifest",
      status: "ready",
    };
  }

  const issue = issues.find((candidate) =>
    candidate.message.startsWith(`${module.name} `)
  );
  const reason = issue?.message ?? "registry doctor snapshot needs attention";

  if (!module.baseUrl) {
    return {
      reason,
      status: "needs_base_url",
    };
  }

  if (
    module.manifestStatus !== "ok" ||
    module.manifestName !== module.name ||
    module.manifestVersion !== module.catalogVersion
  ) {
    return {
      reason,
      status: "manifest_mismatch",
    };
  }

  return {
    reason,
    status: "package_hint_mismatch",
  };
}

function consolePackageKey(hint: AvailableModuleConsolePackageHint): string {
  return `${hint.packageName}#${hint.exportName}`;
}

function normalizeInstallPolicy(
  installPolicy: AvailableModuleRegistryInstallPolicy | undefined
): AvailableModuleRegistryInstallPolicy {
  return installPolicy ?? "review_required";
}
