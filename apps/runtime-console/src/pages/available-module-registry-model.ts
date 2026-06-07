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
  installPolicy?: AvailableModuleRegistryInstallPolicy;
  provenance?: AvailableModuleRegistryProvenance;
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

export type AvailableModuleRegistryProvenance = {
  checksum?: string;
  packageUrl?: string;
  publisher?: string;
  publicKey?: string;
  publicKeyId?: string;
  signatureAlgorithm?: string;
  signatureUrl?: string;
  sourceRepository?: string;
};

export type AvailableModuleRegistryDoctorModule = {
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
  installPolicy?: AvailableModuleRegistryInstallPolicy;
  manifestName: string | null;
  manifestStatus: "ok" | "invalid" | "unreadable" | string;
  manifestVersion: string | null;
  provenance?: AvailableModuleRegistryProvenance;
  status: "ready" | "needs_attention" | string;
};

export type AvailableModulePreflightStatus =
  | "unknown"
  | "archived"
  | "ready"
  | "review_required"
  | "compatibility_blocked"
  | "provenance_blocked"
  | "publisher_trust_blocked"
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
  preflightFix?: string;
  preflightReason: string;
  provenanceChecksum: string;
  provenancePublisher: string;
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
  provenance_blocked: "provenance required",
  publisher_trust_blocked: "publisher trust",
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
      ...(preflight.fix ? { preflightFix: preflight.fix } : {}),
      preflightLabel: statusLabel[preflight.status],
      preflightReason: preflight.reason,
      preflightStatus: preflight.status,
      provenanceChecksum: entry.provenance?.checksum ?? "-",
      provenancePublisher: entry.provenance?.publisher ?? "-",
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
      ...(preflight.fix ? { preflightFix: preflight.fix } : {}),
      preflightLabel: statusLabel[preflight.status],
      preflightReason: preflight.reason,
      preflightStatus: preflight.status,
      provenanceChecksum: module.provenance?.checksum ?? "-",
      provenancePublisher: module.provenance?.publisher ?? "-",
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
      fix: registryRestoreCommand(entry.name),
      reason: entry.archiveReason
        ? `catalog entry archived: ${entry.archiveReason}`
        : "catalog entry is archived",
      status: "archived",
    };
  }

  if (normalizeInstallPolicy(entry.installPolicy) !== "trusted") {
    return {
      reason:
        "registry install requires installPolicy trusted after operator review",
      status: "review_required",
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

  const provenanceIssue = registryProvenanceIssue({
    moduleName: entry.name,
    provenance: entry.provenance,
  });
  if (provenanceIssue) {
    return {
      reason: provenanceIssue,
      status: "provenance_blocked",
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
}): { fix?: string; reason: string; status: AvailableModulePreflightStatus } {
  if (module.status === "archived" || module.archivedAt) {
    return {
      fix: registryRestoreCommand(module.name),
      reason: module.archiveReason
        ? `catalog entry archived: ${module.archiveReason}`
        : "catalog entry is archived",
      status: "archived",
    };
  }

  if (normalizeInstallPolicy(module.installPolicy) !== "trusted") {
    const issue = issues.find(
      (candidate) =>
        candidate.group === "Catalog" &&
        candidate.message.startsWith(`${module.name} installPolicy `)
    );
    return {
      ...(issue?.fix ? { fix: issue.fix } : {}),
      reason:
        issue?.message ??
        "registry install requires installPolicy trusted after operator review",
      status: "review_required",
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

  const provenanceIssue = issues.find(
    (candidate) =>
      candidate.group === "Provenance" &&
      candidate.message.startsWith(`${module.name} `)
  );
  if (provenanceIssue) {
    const publisherTrustFix = modulePublisherTrustFix({
      issue: provenanceIssue,
      module,
    });
    if (publisherTrustFix) {
      return {
        fix: publisherTrustFix,
        reason: provenanceIssue.message,
        status: "publisher_trust_blocked",
      };
    }
    return {
      ...(provenanceIssue.fix ? { fix: provenanceIssue.fix } : {}),
      reason: provenanceIssue.message,
      status: "provenance_blocked",
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

function modulePublisherTrustFix({
  issue,
  module,
}: {
  issue: AvailableModuleRegistryDoctorIssue;
  module: AvailableModuleRegistryDoctorModule;
}): string | null {
  if (issue.fix.startsWith("lenso module publisher ")) {
    return issue.fix;
  }
  const publisher = module.provenance?.publisher;
  const publicKeyId = module.provenance?.publicKeyId;
  if (!(publisher && publicKeyId && issue.message.includes("publisher key"))) {
    return null;
  }
  return `lenso module publisher trust ${quoteCliArgument(publisher)} ${quoteCliArgument(publicKeyId)} --public-key-file <pem>`;
}

function quoteCliArgument(value: string): string {
  return `"${value.replaceAll(/["\\]/gu, "\\$&")}"`;
}

function registryRestoreCommand(moduleName: string): string {
  return `lenso module registry restore ${quoteCliArgument(moduleName)} --reason <reason>`;
}

function consolePackageKey(hint: AvailableModuleConsolePackageHint): string {
  return `${hint.packageName}#${hint.exportName}`;
}

function normalizeInstallPolicy(
  installPolicy: AvailableModuleRegistryInstallPolicy | undefined
): AvailableModuleRegistryInstallPolicy {
  return installPolicy ?? "review_required";
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

function registryProvenanceIssue({
  moduleName,
  provenance,
}: {
  moduleName: string;
  provenance: AvailableModuleRegistryProvenance | undefined;
}): string | null {
  if (!provenance?.publisher) {
    return `${moduleName} provenance publisher is missing`;
  }
  if (!provenance.sourceRepository) {
    return `${moduleName} provenance source repository is missing`;
  }
  if (!provenance.checksum) {
    return `${moduleName} provenance checksum is missing`;
  }
  if (!provenance.signatureUrl) {
    return `${moduleName} provenance signature URL is missing`;
  }
  if (!provenance.publicKeyId) {
    return `${moduleName} provenance public key id is missing`;
  }
  if (!provenance.signatureAlgorithm) {
    return `${moduleName} provenance signature algorithm is missing`;
  }
  if (provenance.signatureAlgorithm !== "ed25519-detached") {
    return `${moduleName} provenance signature algorithm ${provenance.signatureAlgorithm} is unsupported`;
  }
  return null;
}
