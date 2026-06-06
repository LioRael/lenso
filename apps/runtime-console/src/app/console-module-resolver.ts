import { installedConsolePackages } from "../console-package-installs";
import type {
  ConsoleModule,
  ConsoleNavigationMetadata,
  ConsoleSurfaceArea,
} from "./console-module-api";
import {
  consolePackageKey,
  consolePackageRegistryByKey,
  type InstalledConsolePackage,
} from "./console-package-registry";

export type ConsoleModulePackageReference = {
  packageName: string;
  exportName: string;
  navigation?: ConsoleNavigationMetadata;
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
  const navigation = reference.navigation;
  if (!navigation) {
    return registryItem.module;
  }
  return {
    ...registryItem.module,
    surfaces: registryItem.module.surfaces.map((surface) => ({
      ...surface,
      navigation,
    })),
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
      if (surface.navigation) {
        reference.navigation = surface.navigation;
      }
      if (!consolePackageExportIsRegistered(reference)) {
        return [];
      }
      return [reference];
    })
  );
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
