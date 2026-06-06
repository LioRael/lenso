import { installedConsolePackages } from "../console-package-installs";
import type {
  ConsoleModule,
  ConsoleNavigationMetadata,
  ConsoleSurfaceArea,
  ConsoleSurfaceIcon,
} from "./console-module-api";
import {
  consolePackageKey,
  consolePackageRegistryByKey,
  type InstalledConsolePackage,
} from "./console-package-registry";

export type ConsoleModulePackageReference = {
  packageName: string;
  exportName: string;
  moduleName?: string;
  surfaceName?: string;
  label?: string;
  area?: ConsoleSurfaceArea;
  route?: string;
  icon?: string | null;
  navigation?: ConsoleNavigationMetadata | null;
};

export type ConsoleModuleMetadata = {
  module_name?: string;
  console?: {
    name?: string;
    label?: string;
    area?: ConsoleSurfaceArea;
    route?: string;
    package?: {
      name?: string;
      export?: string;
    };
    required_capabilities?: readonly string[];
    icon?: string | null;
    navigation?: ConsoleNavigationMetadata;
  }[];
};

export type ConsoleModuleSelectionOptions = {
  availableCapabilities?: readonly string[];
};

export type MissingConsolePackageReference = {
  key: string;
  moduleName: string;
  surfaceName: string;
  surfaceLabel: string;
  route: string;
  packageName: string;
  exportName: string;
  requiredCapabilities: string[];
};

export type ConsolePackageInstallPlan = {
  key: string;
  packageName: string;
  exportName: string;
  status: "planned";
  reason: string;
  request: ConsolePackageInstallRequest;
};

export type ConsolePackageInstallRequest = {
  packageName: string;
  exportName: string;
  requestedByModule: string;
  route: string;
};

export type ConsolePackageInstallResult = {
  key: string;
  packageName: string;
  exportName: string;
  request: ConsolePackageInstallRequest;
  status: "not_configured" | "requires_manual_install";
  message: string;
  command?: string;
};

export type ConsolePackageInstaller = {
  install(
    plan: ConsolePackageInstallPlan
  ): Promise<ConsolePackageInstallResult>;
};

export function consolePackageExportIsRegistered(
  reference: ConsoleModulePackageReference,
  packages: readonly InstalledConsolePackage[] = installedConsolePackages
): boolean {
  return Boolean(registeredConsolePackage(reference, packages));
}

export function registeredConsolePackage(
  reference: ConsoleModulePackageReference,
  packages: readonly InstalledConsolePackage[] = installedConsolePackages
): InstalledConsolePackage | undefined {
  return consolePackageRegistryByKey(packages)[consolePackageKey(reference)];
}

export function resolveConsoleModule(
  reference: ConsoleModulePackageReference,
  packages: readonly InstalledConsolePackage[] = installedConsolePackages
): ConsoleModule {
  const key = consolePackageKey(reference);
  const registryItem = consolePackageRegistryByKey(packages)[key];
  if (!registryItem) {
    throw new Error(`Console module package export is not registered: ${key}`);
  }
  if (!referenceHasBackendSurface(reference)) {
    return registryItem.module;
  }
  const matchedSurface = matchedConsolePackageSurface(
    registryItem.module,
    reference
  );
  if (!matchedSurface) {
    throw new Error(
      `Console module package export does not include requested surface: ${key} (${surfaceReferenceLabel(reference)})`
    );
  }
  return {
    ...registryItem.module,
    id: reference.moduleName ?? registryItem.module.id,
    surfaces: [consoleSurfaceFromBackendReference(matchedSurface, reference)],
  };
}

export function resolveConsoleModules(
  references: ConsoleModulePackageReference[],
  packages: readonly InstalledConsolePackage[] = installedConsolePackages
): ConsoleModule[] {
  return references.map((reference) =>
    resolveConsoleModule(reference, packages)
  );
}

export function selectConsoleModulePackageReferences(
  modules: ConsoleModuleMetadata[],
  options: ConsoleModuleSelectionOptions = {}
): ConsoleModulePackageReference[] {
  const availableCapabilities = options.availableCapabilities
    ? new Set(options.availableCapabilities)
    : null;
  return modules.flatMap((module) =>
    (module.console ?? []).flatMap((surface) => {
      const packageName = surface.package?.name;
      const exportName = surface.package?.export;
      if (!(packageName && exportName)) {
        return [];
      }
      const requiredCapabilities = surface.required_capabilities ?? [];
      if (
        availableCapabilities &&
        !requiredCapabilities.every((capability) =>
          availableCapabilities.has(capability)
        )
      ) {
        return [];
      }
      const reference: ConsoleModulePackageReference = {
        exportName,
        packageName,
      };
      if (module.module_name) {
        reference.moduleName = module.module_name;
      }
      if (surface.name) {
        reference.surfaceName = surface.name;
      }
      if (surface.label) {
        reference.label = surface.label;
      }
      if (surface.area) {
        reference.area = surface.area;
      }
      if (surface.route) {
        reference.route = surface.route;
      }
      if (isConsoleSurfaceIcon(surface.icon)) {
        reference.icon = surface.icon;
      }
      reference.navigation = surface.navigation ?? null;
      if (!consolePackageExportIsRegistered(reference)) {
        return [];
      }
      return [reference];
    })
  );
}

function referenceHasBackendSurface(
  reference: ConsoleModulePackageReference
): boolean {
  return (
    reference.moduleName !== undefined ||
    reference.surfaceName !== undefined ||
    reference.label !== undefined ||
    reference.area !== undefined ||
    reference.route !== undefined ||
    reference.icon !== undefined ||
    "navigation" in reference
  );
}

function matchedConsolePackageSurface(
  module: ConsoleModule,
  reference: ConsoleModulePackageReference
): ConsoleModule["surfaces"][number] | undefined {
  if (reference.route !== undefined) {
    return module.surfaces.find((surface) => surface.path === reference.route);
  }
  return module.surfaces.length === 1 ? module.surfaces[0] : undefined;
}

function consoleSurfaceFromBackendReference(
  surface: ConsoleModule["surfaces"][number],
  reference: ConsoleModulePackageReference
): ConsoleModule["surfaces"][number] {
  const resolvedSurface = { ...surface };
  if (reference.label !== undefined) {
    resolvedSurface.label = reference.label;
  }
  if (reference.area !== undefined) {
    resolvedSurface.area = reference.area;
  }
  if (reference.route !== undefined) {
    resolvedSurface.path = reference.route;
  }
  if (isConsoleSurfaceIcon(reference.icon)) {
    resolvedSurface.icon = reference.icon;
  } else if (reference.icon === null) {
    delete resolvedSurface.icon;
  }
  if ("navigation" in reference) {
    if (reference.navigation === null || reference.navigation === undefined) {
      delete resolvedSurface.navigation;
    } else {
      resolvedSurface.navigation = reference.navigation;
    }
  }
  return resolvedSurface;
}

function isConsoleSurfaceIcon(
  icon: string | null | undefined
): icon is ConsoleSurfaceIcon {
  return (
    icon === "activity" ||
    icon === "boxes" ||
    icon === "database" ||
    icon === "network" ||
    icon === "settings" ||
    icon === "workflow"
  );
}

function surfaceReferenceLabel(
  reference: ConsoleModulePackageReference
): string {
  return reference.route ?? reference.surfaceName ?? "unknown surface";
}

export function missingConsolePackageReferences(
  modules: ConsoleModuleMetadata[]
): MissingConsolePackageReference[] {
  return modules.flatMap((module) =>
    (module.console ?? []).flatMap((surface) => {
      const packageName = surface.package?.name;
      const exportName = surface.package?.export;
      if (!(packageName && exportName)) {
        return [];
      }
      const reference = { exportName, packageName };
      if (consolePackageExportIsRegistered(reference)) {
        return [];
      }
      return [
        {
          exportName,
          key: consolePackageKey(reference),
          moduleName: module.module_name ?? "unknown",
          packageName,
          requiredCapabilities: [...(surface.required_capabilities ?? [])],
          route: surface.route ?? "-",
          surfaceLabel: surface.label ?? surface.name ?? "-",
          surfaceName: surface.name ?? "-",
        },
      ];
    })
  );
}

export function planConsolePackageInstall(
  missingPackages: MissingConsolePackageReference[]
): ConsolePackageInstallPlan[] {
  return missingPackages.map((missingPackage) => ({
    exportName: missingPackage.exportName,
    key: missingPackage.key,
    packageName: missingPackage.packageName,
    reason: `${missingPackage.moduleName} / ${missingPackage.surfaceLabel} / ${missingPackage.route}`,
    request: {
      exportName: missingPackage.exportName,
      packageName: missingPackage.packageName,
      requestedByModule: missingPackage.moduleName,
      route: missingPackage.route,
    },
    status: "planned",
  }));
}

export function createNoopConsolePackageInstaller(): ConsolePackageInstaller {
  return {
    async install(plan) {
      return {
        exportName: plan.exportName,
        key: plan.key,
        message: "console package installation is not configured",
        packageName: plan.packageName,
        request: plan.request,
        status: "not_configured",
      };
    },
  };
}

export function createDevManualConsolePackageInstaller(): ConsolePackageInstaller {
  return {
    async install(plan) {
      return {
        command: `pnpm --dir apps/runtime-console add ${plan.packageName}`,
        exportName: plan.exportName,
        key: plan.key,
        message: "manual dev install required",
        packageName: plan.packageName,
        request: plan.request,
        status: "requires_manual_install",
      };
    },
  };
}
