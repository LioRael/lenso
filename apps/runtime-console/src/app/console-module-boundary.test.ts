import { describe, expect, test } from "vitest";

describe("console module boundaries", () => {
  test("keeps story-console internals behind the module entrypoint", () => {
    expect(findConsoleModuleBoundaryViolations()).toEqual([]);
  });

  test("covers installed console packages with boundary checks", () => {
    expect(consolePackageNames()).toEqual(installedConsolePackageNames());
  });
});

const sourceFiles = import.meta.glob<string>("../**/*.{ts,tsx}", {
  eager: true,
  import: "default",
  query: "?raw",
});
const consolePackageFiles = import.meta.glob<string>(
  "../../packages/*/src/**/*.{ts,tsx}",
  {
    eager: true,
    import: "default",
    query: "?raw",
  }
);
const consoleSurfaceFiles = import.meta.glob<string>(
  "../../packages/*/console-surface.json",
  {
    eager: true,
    import: "default",
    query: "?raw",
  }
);
const modulePrefix = "../modules/";
const consolePackagePrefix = "../../packages/";
const hostApiPackagePrefix = "../../packages/console-package-api/src/";
const storyPackagePrefix = "../../packages/story-console/src/";
const storyModulePrefix = "../modules/story-console/";
const importPattern =
  /\b(?:import|export)\s+(?:type\s+)?(?:[^'"]*?\s+from\s+)?["']([^"']+)["']/g;
const runtimeConsolePackageJson =
  Object.values(
    import.meta.glob<{ dependencies?: Record<string, string> }>(
      "../../package.json",
      {
        eager: true,
        import: "default",
      }
    )
  )[0] ?? {};

function findConsoleModuleBoundaryViolations(): string[] {
  const violations: string[] = [];

  for (const [file, source] of Object.entries({
    ...sourceFiles,
    ...consolePackageFiles,
  })) {
    const inConsolePackage = file.startsWith(consolePackagePrefix);
    const inHostApiPackage = file.startsWith(hostApiPackagePrefix);
    const inInstalledConsolePackage = inConsolePackage && !inHostApiPackage;
    const inConsoleModule =
      file.startsWith(modulePrefix) || inInstalledConsolePackage;
    const inStoryModule =
      file.startsWith(storyModulePrefix) || file.startsWith(storyPackagePrefix);

    for (const specifier of importSpecifiers(source)) {
      const target = resolveImport(file, specifier);

      if (inConsoleModule && target.includes("/app/")) {
        violations.push(
          `${displayPath(file)} imports host app internals through ${specifier}`
        );
      }

      if (
        inConsoleModule &&
        target === "@lenso/runtime-console/console-package-api"
      ) {
        violations.push(
          `${displayPath(file)} imports host API through ${specifier}; use @lenso/runtime-console-api`
        );
      }

      if (inStoryModule && target.includes("/pages/")) {
        violations.push(
          `${displayPath(file)} imports host pages through ${specifier}`
        );
      }

      if (inStoryModule && target.includes("/hooks/")) {
        violations.push(
          `${displayPath(file)} imports host hooks through ${specifier}`
        );
      }

      if (inStoryModule && target.endsWith("/runtime-console-context")) {
        violations.push(
          `${displayPath(file)} imports host runtime context through ${specifier}`
        );
      }

      if (inStoryModule && target.includes("/components/runtime/")) {
        violations.push(
          `${displayPath(file)} imports host runtime UI through ${specifier}`
        );
      }

      if (inStoryModule && target.includes("/components/ui/")) {
        violations.push(
          `${displayPath(file)} imports host common UI through ${specifier}`
        );
      }

      if (inStoryModule && target.includes("/data/")) {
        violations.push(
          `${displayPath(file)} imports host data through ${specifier}`
        );
      }

      if (
        !inStoryModule &&
        (target === "@lenso/story-console" ||
          target === "../modules/story-console" ||
          target.startsWith(storyModulePrefix)) &&
        target !== "@lenso/story-console"
      ) {
        violations.push(
          `${displayPath(file)} imports story-console internals through ${specifier}`
        );
      }

      if (
        !inConsolePackage &&
        target.startsWith(consolePackagePrefix) &&
        !target.includes("/console-package-api/")
      ) {
        violations.push(
          `${displayPath(file)} imports console package internals through ${specifier}`
        );
      }
    }
  }

  return violations.sort();
}

function consolePackageNames(): string[] {
  return [
    ...new Set(
      Object.keys(consoleSurfaceFiles).flatMap((file) => {
        const packageName = file.match(
          /^\.\.\/\.\.\/packages\/([^/]+)\//u
        )?.[1];
        return packageName ? [packageName] : [];
      })
    ),
  ].sort();
}

function installedConsolePackageNames(): string[] {
  return Object.keys(runtimeConsolePackageJson.dependencies ?? {})
    .filter(
      (name) =>
        name.startsWith("@lenso/") && name !== "@lenso/runtime-console-api"
    )
    .map((name) => name.replace("@lenso/", ""))
    .sort();
}

function importSpecifiers(source: string): string[] {
  return [...source.matchAll(importPattern)].map((match) => match[1] ?? "");
}

function resolveImport(importer: string, specifier: string): string {
  if (!specifier.startsWith(".")) {
    return specifier;
  }

  const importerParts = importer.split("/");
  importerParts.pop();
  for (const part of specifier.split("/")) {
    if (part === "." || part === "") {
      continue;
    }
    if (part === "..") {
      importerParts.pop();
      continue;
    }
    importerParts.push(part);
  }
  return importerParts.join("/");
}

function displayPath(path: string): string {
  return path.replace(/^\.\.\//u, "");
}
