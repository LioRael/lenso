import { storyConsoleModule } from "../modules/story-console";
import type { ConsoleModule } from "./console-module-api";

export type ConsoleModulePackageReference = {
  packageName: string;
  exportName: string;
};

export type ConsoleModuleMetadata = {
  module_name?: string;
  console?: {
    name?: string;
    label?: string;
    route?: string;
    package?: {
      name?: string;
      export?: string;
    };
    required_capabilities?: readonly string[];
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
};

export type ConsolePackageInstallResult = {
  key: string;
  packageName: string;
  exportName: string;
  status: "not_configured";
  message: string;
};

export type ConsolePackageInstaller = {
  install(
    plan: ConsolePackageInstallPlan
  ): Promise<ConsolePackageInstallResult>;
};

const firstPartyConsoleModuleExports: Record<string, ConsoleModule> = {
  "@lenso/story-console#storyConsoleModule": storyConsoleModule,
};

function packageExportKey(reference: ConsoleModulePackageReference): string {
  return `${reference.packageName}#${reference.exportName}`;
}

export function consolePackageExportIsRegistered(
  reference: ConsoleModulePackageReference
): boolean {
  return Boolean(firstPartyConsoleModuleExports[packageExportKey(reference)]);
}

export function resolveConsoleModule(
  reference: ConsoleModulePackageReference
): ConsoleModule {
  const key = packageExportKey(reference);
  const module = firstPartyConsoleModuleExports[key];
  if (!module) {
    throw new Error(`Console module package export is not registered: ${key}`);
  }
  return module;
}

export function resolveConsoleModules(
  references: ConsoleModulePackageReference[]
): ConsoleModule[] {
  return references.map(resolveConsoleModule);
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
      const reference = { exportName, packageName };
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
          key: packageExportKey(reference),
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
        status: "not_configured",
      };
    },
  };
}
