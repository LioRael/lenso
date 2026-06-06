import { storyConsoleModule } from "../modules/story-console";
import type { ConsoleModule } from "./console-module-api";

export type ConsoleModulePackageReference = {
  packageName: string;
  exportName: string;
};

export type ConsoleModuleMetadata = {
  console?: {
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
