export type AvailableModuleRegistryCatalog = {
  version: number;
  modules: AvailableModuleRegistryEntry[];
};

export type AvailableModuleRegistryEntry = {
  name: string;
  version: string;
  source: "remote" | string;
  manifestReference: string;
  archivedAt?: string;
  archiveReason?: string;
  baseUrl?: string;
  capabilities?: string[];
  consolePackages?: AvailableModuleConsolePackageHint[];
  compatibility?: AvailableModuleRegistryCompatibility;
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

export type AvailableModuleRegistrySnapshot = {
  version: number;
  status: "passed" | "failed" | string;
  catalog: {
    modules: number;
    registryFile: string;
    version: number;
  };
  issues: AvailableModuleRegistryIssue[];
  modules: AvailableModuleRegistryModule[];
};

export type AvailableModuleRegistryIssue = {
  group: string;
  message: string;
  fix?: string;
};

export type AvailableModuleRegistryCompatibility = {
  consolePackageApi?: string;
  lenso?: {
    maxVersion?: string;
    minVersion?: string;
  };
};

export type AvailableModuleRegistryHostCompatibility = {
  consolePackageApi: string;
  lensoVersion: string;
};

export type AvailableModuleRegistryModule = {
  name: string;
  source: "remote" | string;
  catalogVersion: string;
  manifestReference: string;
  archivedAt?: string;
  archiveReason?: string;
  baseUrl: string | null;
  consolePackageHints: number;
  compatibility?: AvailableModuleRegistryCompatibility;
  hostCompatibility?: AvailableModuleRegistryHostCompatibility;
  manifestName: string | null;
  manifestStatus: "ok" | "invalid" | "unreadable" | string;
  manifestVersion: string | null;
  status: "ready" | "needs_attention" | string;
};

export type AvailableModulePreflightStatus =
  | "unknown"
  | "archived"
  | "ready"
  | "compatibility_blocked"
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
  preflightFix?: string;
  preflightReason: string;
  summary: string;
};

export type AvailableModuleManifestSnapshots = Record<
  string,
  AvailableModuleManifestSnapshot | undefined
>;

const statusLabel: Record<AvailableModulePreflightStatus, string> = {
  archived: "archived",
  compatibility_blocked: "incompatible",
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
      ...(preflight.fix ? { preflightFix: preflight.fix } : {}),
      preflightLabel: statusLabel[preflight.status],
      preflightReason: preflight.reason,
      preflightStatus: preflight.status,
      source: entry.source,
      summary: entry.summary ?? "-",
      version: entry.version,
    };
  });
}

export function availableModuleRegistryRowsFromSnapshot(
  snapshot: AvailableModuleRegistrySnapshot
): AvailableModuleRegistryRow[] {
  return snapshot.modules.map((module) => {
    const preflight = availableModulePreflightFromSnapshot({
      issues: snapshot.issues,
      module,
    });
    return {
      baseUrl: module.baseUrl ?? "-",
      capabilityCount: 0,
      consolePackageHintCount: module.consolePackageHints,
      key: `${module.name}:${module.catalogVersion}:${module.manifestReference}`,
      manifestReference: module.manifestReference,
      name: module.name,
      ...(preflight.fix ? { preflightFix: preflight.fix } : {}),
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
): { fix?: string; reason: string; status: AvailableModulePreflightStatus } {
  if (entry.archivedAt) {
    return {
      reason: entry.archiveReason
        ? `catalog entry archived: ${entry.archiveReason}`
        : "catalog entry is archived",
      status: "archived",
    };
  }

  const compatibilityIssue = registryCompatibilityIssue({
    compatibility: entry.compatibility,
    hostCompatibility: defaultHostCompatibility,
    moduleName: entry.name,
  });
  if (compatibilityIssue) {
    return {
      reason: compatibilityIssue,
      status: "compatibility_blocked",
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
      reason: "manifest will be read from the manifest URL during install",
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
    reason: "module manifest is available",
    status: "ready",
  };
}

function availableModulePreflightFromSnapshot({
  issues,
  module,
}: {
  issues: AvailableModuleRegistryIssue[];
  module: AvailableModuleRegistryModule;
}): { fix?: string; reason: string; status: AvailableModulePreflightStatus } {
  if (module.status === "archived" || module.archivedAt) {
    return {
      reason: module.archiveReason
        ? `catalog entry archived: ${module.archiveReason}`
        : "catalog entry is archived",
      status: "archived",
    };
  }

  const compatibilityIssue = issues.find(
    (candidate) =>
      candidate.group === "Compatibility" &&
      candidate.message.startsWith(`${module.name} `)
  );
  if (compatibilityIssue) {
    return {
      ...(compatibilityIssue.fix ? { fix: compatibilityIssue.fix } : {}),
      reason: compatibilityIssue.message,
      status: "compatibility_blocked",
    };
  }

  if (module.status === "ready") {
    return {
      reason: "module manifest is available",
      status: "ready",
    };
  }

  const issue = issues.find((candidate) =>
    candidate.message.startsWith(`${module.name} `)
  );
  const reason = issue?.message ?? "module manifest needs attention";

  if (!module.baseUrl) {
    return {
      ...(issue?.fix ? { fix: issue.fix } : {}),
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
      ...(issue?.fix ? { fix: issue.fix } : {}),
      reason,
      status: "manifest_mismatch",
    };
  }

  return {
    ...(issue?.fix ? { fix: issue.fix } : {}),
    reason,
    status: "package_hint_mismatch",
  };
}

function consolePackageKey(hint: AvailableModuleConsolePackageHint): string {
  return `${hint.packageName}#${hint.exportName}`;
}

const defaultHostCompatibility: AvailableModuleRegistryHostCompatibility = {
  consolePackageApi: "1",
  lensoVersion: "0.1.0",
};

function parseVersion(value: string): [number, number, number] | null {
  const match = /^(\d+)\.(\d+)\.(\d+)$/u.exec(value);
  if (!match) {
    return null;
  }
  return [Number(match[1]), Number(match[2]), Number(match[3])];
}

function compareVersions(left: string, right: string): number | null {
  const leftParts = parseVersion(left);
  const rightParts = parseVersion(right);
  if (!(leftParts && rightParts)) {
    return null;
  }
  for (let index = 0; index < leftParts.length; index += 1) {
    const leftPart = leftParts[index] ?? 0;
    const rightPart = rightParts[index] ?? 0;
    if (leftPart !== rightPart) {
      return leftPart > rightPart ? 1 : -1;
    }
  }
  return 0;
}

function registryCompatibilityIssue({
  compatibility,
  hostCompatibility,
  moduleName,
}: {
  compatibility: AvailableModuleRegistryCompatibility | undefined;
  hostCompatibility: AvailableModuleRegistryHostCompatibility;
  moduleName: string;
}): string | null {
  const minVersion = compatibility?.lenso?.minVersion;
  if (minVersion) {
    const comparison = compareVersions(
      hostCompatibility.lensoVersion,
      minVersion
    );
    if (comparison === null || comparison < 0) {
      return `${moduleName} requires Lenso >= ${minVersion}; host is ${hostCompatibility.lensoVersion}`;
    }
  }
  const maxVersion = compatibility?.lenso?.maxVersion;
  if (maxVersion) {
    const comparison = compareVersions(
      hostCompatibility.lensoVersion,
      maxVersion
    );
    if (comparison === null || comparison > 0) {
      return `${moduleName} supports Lenso <= ${maxVersion}; host is ${hostCompatibility.lensoVersion}`;
    }
  }
  if (
    compatibility?.consolePackageApi &&
    compatibility.consolePackageApi !== hostCompatibility.consolePackageApi
  ) {
    return `${moduleName} requires console package API ${compatibility.consolePackageApi}; host supports ${hostCompatibility.consolePackageApi}`;
  }
  return null;
}
