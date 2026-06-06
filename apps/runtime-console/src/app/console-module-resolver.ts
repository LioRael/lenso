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
  }[];
};

const firstPartyConsoleModuleExports: Record<string, ConsoleModule> = {
  "@lenso/story-console#storyConsoleModule": storyConsoleModule,
};

function packageExportKey(reference: ConsoleModulePackageReference): string {
  return `${reference.packageName}#${reference.exportName}`;
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
  modules: ConsoleModuleMetadata[]
): ConsoleModulePackageReference[] {
  return modules.flatMap((module) =>
    (module.console ?? []).flatMap((surface) => {
      const packageName = surface.package?.name;
      const exportName = surface.package?.export;
      if (!(packageName && exportName)) {
        return [];
      }
      const reference = { exportName, packageName };
      if (!firstPartyConsoleModuleExports[packageExportKey(reference)]) {
        return [];
      }
      return [reference];
    })
  );
}
